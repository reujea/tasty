# Markdown Viewer (`com.tasty.markdown`)

- **Status**: Implemented (bundled plugin)
- **주체**: 로컬 사용자 (GUI surface) · AI Agent (`tasty markdown` CLI)
- **배포/통합**: bundled · surface_kind(webview) · 파일 핸들러 — [plugins 개념](../../concepts/plugins.md)
- **코드**: `crates/tasty-plugin-markdown/`(`crates/tasty-plugin-markdown/src/render.rs` = `pulldown-cmark` → sanitize(`ammonia`) → CSS 주입 HTML 문서 생성, `crates/tasty-plugin-markdown/src/main.rs` surface 라이프사이클/네비게이션, `crates/tasty-plugin-markdown/src/popup.rs` egui-mesh 팝업(file-open/large-file-confirm),
  idle 자동 리로드는 SDK 공용 `crates/tasty-plugin-sdk/src/file_watch.rs`) — [WebView 호스트 계약](../../design/systems/webview.md)(EguiMesh→Webview 전환 결정) · [egui-mesh 렌더 채널](../../dev-guide/egui-mesh-channel.md)(egui-mesh 채널, 팝업만 계속 사용)
- **권한**: `surface.read/write`, `fs.read`(파일 읽기 + 링크 dispatch), `file_handler.*`, `ui.settings_page`, `ui.popup`(file-open/large-file-confirm 만 자가 렌더 — 본문은 아님) (매니페스트 `permissions`)
- **화면**: [screens/markdown.md](screens/viewer.md)

> **예제로서**: webview surface(plugin 이 sanitize 된 HTML 문서를 생성, host native WebView 가 렌더) + **파일 detector/handler** + cli + settings_page 를 한 플러그인에 모은 예제 → [plugin-development](../../dev-guide/plugin-development.md#파일-핸들러-detector--handler).

## 목적

마크다운 파일을 렌더해 보는 **`markdown` surface 종류**를 제공한다. `rendering = "webview"`([WebView 호스트 계약](../../design/systems/webview.md)) — 플러그인이 `pulldown-cmark` 로 markdown 을 HTML 로 변환하고 `ammonia` 로 sanitize 한 뒤, Theme 토큰을 CSS custom property 로 주입한 `<style>` 을 문서에 인라인해 host 의 native OS WebView(WebKitGTK/WKWebView/WebView2) overlay 에 올린다(`webview.set_url`). 같은 호출의 `label` 에 문서 경로를 보내, WebView 가 숨겨진 동안 host chrome 이 HTML 원문 대신 경로를 보인다(경로 없는 문서는 label 을 빼고 chrome 의 URL 줄도 비운다). 문서의 픽셀은 WebView가 렌더하며 host는 mesh를 합성하지 않는다.

## 내부 동작

- **surface_kind `markdown` (webview)** — plugin 이 `MdDoc`(파일 경로·내용·base_dir·load_error)를 소유하고, `render::render_document` 가 완전한 `<!doctype html>` 문서 하나를 생성한다: `<style>`(CSS custom property, Theme→토큰) + 주소창 `<input>`+`<button>` + 목차 `<nav>` + 본문(sanitize 된 HTML) + 신뢰된 인라인 `<script>`(주소창 Go/Enter + 문서 안 앵커 스크롤 + 스크롤 복원).
  **`<base href>` 는 없다** — 이 파이프라인이 내보내는 것 중 상대 URL 을 푸는 자리가 없고(이미지는 인라인, **마크다운 링크** destination 은 anchor-only 가 아닌 한 전부 nav fragment), base 가 있으면 fragment-only `href` 가 base URL 기준으로 풀려 목차 클릭이 다른 문서로 나가 버린다(아래 기능 계약).
  저자가 직접 쓴 raw HTML `<a href="x.md">` 는 rewrite 대상이 아니라 상대 href 로 남지만, base 가 없으면 그것은 **파일로 풀리지 않는다**(클릭했을 때 엔진이 무엇을 하는지는 세 백엔드 어디에서도 측정되지 않았다).
  display_name 은 파일명.
  - **제목 크기**: `render::heading_sizes_px` 가 `font-size-prose-h1`(h1)↔`font-size-body`(h6) 사이를 5단계 **선형보간**한다 — CSS 라 라이브러리 제약 없이 이 보간 자체가 plugin 의 디자인 선택이다(원하면 per-level 값을 자유롭게 override 가능).
  - **표(GFM)**: 실제 `<table>`/`<th>`/`<td>` — header 밴드·zebra(`tr:nth-child(even)`)·셀 패딩 전부 CSS 로 직접 달성(egui `Grid` 우회 불필요).
  - **코드블록**: `sanitize_fence_lang`은 언어 태그를 `[A-Za-z0-9_+-]`로 정리하고 `code`의 class를 유지한다. `render_document`는 생성한 본문 HTML에 `class="language-`가 있으면 highlight.js를, `language-mermaid`가 있으면 mermaid.js를 넣는다. 문자열 포함 검사이므로 실제 코드 블록 외의 본문도 이 조건에 걸릴 수 있다. 실행 스크립트가 코드 요소를 선택해 처리하며, Mermaid는 `code.language-mermaid`를 대상으로 한다. 실패는 console error로 기록한다. 모든 OS·오프라인 환경에서 실행을 확인한 것은 아니다.
- **sanitize (XSS 방어의 1차 관문)** — `ammonia::Builder` 최소 화이트리스트: `<script>`/이벤트 핸들러 속성(`onerror=` 등)/`javascript:` scheme href 전부 stripped. `classify_link` 도 별도로 `javascript:` 를 판정 불가(`None`)로 취급해 이중 방어. GFM 렌더에 필요한 태그(`table`/체크박스 `input[type=checkbox]`/`del`/footnote `sup`/`div`/이미지/링크)만 허용 — 코드는 `crates/tasty-plugin-markdown/src/render.rs::sanitize_html` 이 정본.
- **리로드·삭제 처리**: 명시적 `markdown.reload`와 파일 감시 요청은 같은 플러그인 워커에서 처리한다. 로컬 파일을 다시 읽고 HTML을 만들며 읽기에 실패하면 `markdown.state.failed`를 표시한다. 파일이 다시 생긴 것을 감지하면 다시 읽는다. 워커의 직렬 처리가 파일 시스템의 동시 변경까지 막는 것은 아니다. 명시적 `markdown.reload`가 이 플러그인이 연 문서가 아닌 surface ID를 가리키면 invalid params(`Surface <id> is not a markdown surface`)로 거절한다. 감시의 자기 호출은 surface 해제와 엇갈릴 수 있으므로 닫힌 surface를 가리키면 오류 없이 무시한다.
- **debug 검증용 감시 끄기**: debug 빌드의 plugin 은 환경변수 `TASTY_DEBUG_MARKDOWN_NO_WATCH` 에 값이 있으면 파일 감시 워커를 띄우지 않는다. 자동 갱신과 명시적 갱신(공통 새로고침·`markdown.reload`)을 구별해 검증할 때 쓴다. release 빌드에는 이 분기가 없다.
- **공통 새로고침**: `surface.refresh_requested` 를 구독한다(매니페스트 `event_subscribe`). payload 의 `kind` 가 `markdown` 이고 `surface_id` 가 이 plugin 이 연 문서일 때만 처리한다. 로컬 문서는 명시적 reload(`ReloadOrigin::Explicit`)와 같이 대용량 확인을 거쳐 파일을 다시 읽고 다시 렌더한다. mirror 문서는 서버 원문을 사용자 출처(`user_initiated: true`, 문서 안 Reload 와 같다)로 다시 요청한다. 끊김 상태면 host 가 회신하지 않거나 실패를 알리고 문서는 끊김·실패 표시를 유지한다 — 로컬 파일이나 캐시로 대신하지 않는다. 보관 HTML 의 재렌더나 native WebView reload 로 대신하지 않는다. 다른 문서에는 적용하지 않는다. 처리 결과의 별도 알림은 없고 문서 내용이 바뀌는 것으로 보인다. 갱신은 문서 HTML 을 통째로 다시 싣기 때문에 주소칸 draft·검색 바와 검색어·선택·진행 중인 IME 조합은 초기화되고 스크롤만 아래 "스크롤 위치 보존" 규칙으로 best-effort 복원된다(문서 안 Reload 버튼과 같다). 계약은 [공통 새로고침](../../features/keybindings/index.md#공통-새로고침--refresh)에 있다.
- **입력 없는 자동 갱신**: 별도 스레드가 SDK의 `file_watch`를 사용한다([플러그인 개발 가이드](../../dev-guide/plugin-development.md)). 검사 사이의 대기는 `RELOAD_CHECK_INTERVAL_SECS`(1초)이며 조회·읽기 시간을 포함한 전체 갱신 상한은 아니다. 감시에서도 파일을 읽어 내용 해시를 비교하고, 바뀌면 `self_invoke`로 워커의 `markdown.reload`를 요청한다. 자기 namespace를 `host.call`로 부르면 호스트 구현으로 전달되므로 플러그인 내부 호출에는 사용하지 않는다.
- **콘텐츠 전달** — surface 생성 시 host 가 `surface.create{file}` 를 plugin 에 보낸다. plugin 이 파일을 직접 읽는다(`fs.read`).
- **Theme parity** — webview-kind surface 는 Theme 이 자동으로 push 되지 않으므로(egui-mesh 의 `set_context.theme` 와 달리), plugin 이 문서를 (재)생성할 때마다 host 의 read-only **`theme.query`** IPC 로 현재 색+`is_light`+UI zoom 을 직접 조회한다. 이후 테마 ID·기본 색·색 override·라이트 여부·UI 배율 중 하나라도 바뀌면 host 가 발행하는 **`theme.changed`** 이벤트(매니페스트 `event_subscribe`)를 구독해 열려 있는 모든 markdown 문서를 재생성한다. 색 override 나 UI 배율만 바뀐 경우에도 이벤트가 오므로, 문서는 다음 재생성을 기다리지 않고 바로 새 색과 zoom 을 따른다. host 는 새 설정의 전역 Theme 를 설치한 뒤에 `theme.changed` 를 발행하므로, 수신 후 호출한 `theme.query` 는 새 테마를 반환한다. `src/app/dispatch_domain/theme_order.rs` 의 시험은 테마를 여러 번 왕복하며 `install_theme_then` 이 설치를 마친 뒤에 발행 클로저를 실행하는지 확인한다. 소스 가드 `src/source_guards/theme_changed_after_install.rs` 는 본체 `src/` 의 `"theme.changed"` 리터럴이 모두 `announce_settings_change` 안에 있고, 그 호출이 모두 `install_theme_then` 의 클로저 인자 안에 있는지, 발행이 `if appearance_changed {` 블록 안에 있는지 텍스트로 확인한다. 어떤 변경이 발행 대상인지는 같은 파일의 `appearance_changed` 시험이 확인한다. 가드는 클로저를 변수로 받아 넘기는 형태, 상수나 매크로로 키를 감춘 발행, `crates/` 의 발행, 클로저 안의 실제 실행 여부는 판정하지 않는다.
- **JS 는 기본 허용** — host 의 webview 설정 기본값은 plugin 마다 다르다: html 플러그인은 JS 기본 차단(임의 원격 콘텐츠를 열 수 있어서), markdown 은 **기본 허용**(매니페스트 `[surface_kinds.webview_defaults] sandbox_scripts = false`, [WebView 호스트 계약 "설정 출처"](../../design/systems/webview.md#설정-출처--kind-를-등록한-plugin)) — 신뢰된 주소창/네비게이션 스크립트가 항상 실행돼야 하고, 실제 markdown 콘텐츠 자체는 이미 별도로 sanitize 되므로 플러그인이 추가하는 신뢰 스크립트와 저자가 작성한 본문을 구분한다. sanitize가 모든 스크립트 위험을 없앤다고 보장하지 않는다.
- **인라인 이미지**: 정리한 본문의 로컬 `<img src>`를 플러그인이 읽어 `data:` URI로 넣는다. 상대 경로, scheme 없는 절대 경로와 raw HTML 이미지에 같은 처리를 적용한다. WebView가 로컬 파일 경로를 직접 열지 않아도 되게 하기 위한 방식이다.
  `canonicalize`한 경로가 문서 디렉터리 안에 있고 확장자가 PNG/APNG/JPEG/GIF/WebP/AVIF/BMP/ICO/SVG 목록에 있어야 한다. 파일을 읽기 전 metadata 길이로 한 장 4 MiB·문서 총합 16 MiB 기준을 확인한다. 경로 확인·metadata 조회·읽기는 별도 작업이므로 그 사이 파일이나 심볼릭 링크가 바뀌는 경우까지 막는 보장은 아니다. 읽지 못한 이미지의 `src`는 제거해 실패 표시 대상으로 둔다.
  원격 `http(s)` 이미지는 그대로 두며 호스트의 `allow_remote_content` 설정을 따른다. 기본값은 꺼짐이고 Settings › Appearance › Markdown에서 바꾼다. 세 백엔드에 차단 구현이 있으며 실제 차단·허용 비교는 Linux/WebKitGTK에서 확인했다. 다른 OS 실행은 확인하지 않았다. 선택 이유는 아래 기능 계약에 있다.
- **파일 핸들러** — `detector "markdown"`(확장자 매핑) + `handler` action `open_surface{surface_kind:"markdown"}`. 마크다운 파일 열기 시 이 surface 로 뜬다.
- **파일 열기와 대용량 확인 팝업** — 매니페스트에 `file-open`과 `large-file-confirm`을 등록하며 egui-mesh로 그린다. 파일 열기 팝업은 경로 입력, 찾아보기, 열기/취소로 구성한다.

  `file-open`은 `scope = "surface"`다. host가 연결한 대상 surface가 보일 때 그 영역 가운데에 뜬다. 제자리 변환은 그 surface를, 새 탭 열기는 당시 포커스된 surface를 대상으로 삼는다. 다른 workspace나 탭으로 이동하면 숨고 돌아오면 복원된다. event trigger로 열 때는 이벤트가 이 플러그인의 surface를 가리키면 그 surface, 대상이 없으면 창 범위를 사용한다([팝업 범위](../../design/systems/popup.md#plugin-popup-의-스코프)).

  `large-file-confirm`도 `scope = "surface"`다. 대용량 파일을 연 markdown surface 가 `surface_id`를 실은 surface 이벤트를 발행하므로, 확인 팝업은 그 surface 영역 가운데에 뜨고 scrim도 그 surface 만 덮는다. surface 가 팝업 선언 크기(360×150)보다 좁거나 낮으면 창 가운데에 연다. host 는 대상이 이 플러그인의 surface 일 때만 연다.

  크기 기준은 `LARGE_FILE_LIMIT_BYTES`(1 MiB)를 넘는 파일이다. 생성·복원뿐 아니라 `markdown.reload`와 파일 감시도 같은 확인을 거친다(`reload_step`).

  | 문서 상태 | 파일 감시 | 명시적 `markdown.reload` |
  |---|---|---|
  | 확인 대기 또는 취소 | 읽지 않는다 | 기준을 넘으면 다시 묻는다. 이미 확인을 요청했고 그 팝업이 닫히지 않았으면(팝업이 열리기 전 포함, `confirm_requested`) 겹쳐 띄우지 않는다. 기준 아래로 줄었으면 바로 읽는다 |
  | 읽음, [열기]를 고른 적 없음 | 기준을 넘게 커지면 묻는다. 답을 받을 때까지 이전 내용을 그대로 보이고, 취소하면 그때 비워 대기 상태가 되며, [열기]면 새 내용을 읽는다 | 같다 |
  | [열기]를 고른 문서 | 크기와 관계없이 읽는다 | 같다 |

  event bus가 없어 확인을 띄울 수 없으면 생성 때처럼 경고를 남기고 읽는다.

  `markdown.reload` 응답은 `{ok, surface_id, deferred}`다. `deferred`는 로컬 문서가 파일을 읽지 않고 사용자의 대용량 확인을 기다리거나 취소 상태로 남았을 때 `true`, 다시 읽었으면 `false`다. mirror 문서의 응답에는 싣지 않는다.

  | 열기 경로 | 동작 |
  |---|---|
  | `convert_input_popup = "file-open"` | host가 convert 팝업, `open_markdown`·`convert_to_markdown` 단축키, context menu에서 `open_popup_instance`로 연다. |
  | `com.tasty.markdown.file_open` | event trigger로 연다. |

  **찾아보기**는 `file_picker.trigger {filters,owner_popup_instance?,start_dir?,origin_surface_id?} → {request_id}`로 host의 파일 피커를 연다. `fs.read` 권한이 필요하며 선택 결과는 `file_picker.result` 이벤트로 비동기 도착한다. host는 고른 경로로 파일을 열거나 클립보드에 쓰지 않는다. markdown이 받은 경로를 경로 입력칸에 채우고, 열기는 사용자가 팝업에서 확정할 때 한다. 주소창 폴더 버튼은 같은 피커를 팝업 없이 열고 고른 파일로 지금 surface 를 바로 바꾼다(아래 주소창 절).

  시작 폴더는 open context의 로컬 `observed_cwd`, mirror이면 `remote_cwd`를 `start_dir`에 담는다. `origin_surface_id`도 그대로 전달한다. `inherit_cwd` 설정의 영향을 받는 `cwd` 키와는 다르다([파일 피커의 시작 위치](../../features/native-file-picker/index.md)).

  `owner_popup_instance`에 file-open 자신의 인스턴스를 넣어 두 팝업을 부모·자식으로 연결한다. 피커가 떠 있는 동안 부모는 바깥 클릭으로 닫히지 않는다. Esc는 위쪽 피커부터 닫으며, 부모가 먼저 닫히면 피커도 함께 정리한다([내부 팝업 시스템](../../design/systems/popup.md)).

  **열기를 확정하면** 먼저 입력 경로를 정리한 뒤 open context의 `surface_id` 유무에 따라 처리한다.

  - 상대경로는 팝업을 띄운 surface의 셸 cwd 기준 절대경로로 만든다. 찾아보기의 시작 폴더와 같은 값을 쓴다. mirror이면 `remote_cwd`, 로컬이면 `observed_cwd`다. host의 `inherit_cwd` 설정을 따르는 `cwd` 키는 쓰지 않는다.
  - 이미 절대경로면 그대로 보낸다. mirror에서는 로컬 OS 규칙 대신 `/`·`\`·드라이브 문자(`C:`) 접두어로 판정한다. 드라이브 상대경로(`C:a.md`)도 원격 OS가 해석하도록 그대로 둔다.
  - 원격 경로의 구분자는 `remote_cwd`의 모양으로 고른다. 드라이브 문자나 `\\`로 시작하면 `\`, 그 밖은 `/`다. 로컬 `Path::join`을 쓰면 Windows와 Unix가 섞인 attach에서 원격에 맞지 않는 구분자가 붙기 때문이다. 로컬 surface는 로컬 OS의 `Path::join`을 쓴다.
  - cwd를 알 수 없으면(context에 해당 키가 없거나 비어 있음) 조합하지 않고 입력 그대로 보낸다. 짐작한 폴더로 조합하면 사용자가 보지 않은 위치의 파일을 열 수 있고, 거절을 팝업 안에 표시하려면 디자인이 먼저 필요하다. 이때 상대경로는 이전처럼 실제로 파일을 읽는 프로세스의 cwd 기준으로 해석된다.
  - `~`로 시작하는 경로는 조합하지도 확장하지도 않고 그대로 보낸다. `.`·`..`는 정규화하지 않고 읽는 쪽 OS에 맡긴다.
  - 이 규칙은 file-open 팝업의 확정(제자리 변환과 새 탭 열기)에만 적용한다. 주소창 이동과 외부 `markdown.navigate`는 입력을 그대로 보낸다.

  | 조건 | 호출 |
  |---|---|
  | `surface_id` 있음 | `markdown.navigate {surface_id,path,owner_popup_instance}`로 해당 surface를 제자리 변환. host는 로컬 surface이면 로컬 파일 존재를 확인하고, mirror surface이면 원격 경로로 보고 확인 없이 원격에 전달한다. 원격은 그 경로에 파일이 없으면 변환하지 않고 `path not found: <경로>` 로 거절한다. 응답 `{accepted, run}`의 `run`으로 `run.result`가 변환 결과를 돌려준다([파일 핸들러](../../features/file-handler/index.md#접수-뒤-최종-결과)). |
  | `surface_id` 없음 | `file_handler.dispatch {path,depth:"deep",owner_popup_instance}`로 새 탭 열기 |

  두 확정 요청의 `owner_popup_instance`도 file-open 자신의 인스턴스다. host는 `file_handler.dispatch`와 같은 규칙으로 사용자가 조작한 팝업의 요청임을 확인한다. 사용자 요청이면 새 탭을 선택하고, mirror 제자리 변환의 원격 실패를 toast로 알린다. 빠지면 제스처를 증명하지 못한 plugin 요청(PluginUnverified)으로 처리한다. 새 탭을 선택하지 않고 원격 실패도 warn 로그로만 남기지만, 새 탭 요청에 매칭 핸들러가 없으면 fallback picker 를 연다([파일 핸들러 (File handler)](../../features/file-handler/index.md)).

  주소창 이동은 팝업이 없으므로 `markdown.navigate`에 `owner_popup_instance`를 싣지 않는다. 주소창 폴더 버튼은 대신 피커 요청 번호(`picker_request_id`)를 싣는다. 외부 CLI·IPC의 `markdown.navigate`는 plugin이 host로 다시 보내는데, 이때 사용자 출처 증명 필드(`owner_popup_instance`·`picker_request_id`·`user_navigation_url`·`origin_surface_id`)를 모두 제거한다. host는 이 요청의 호출자를 plugin으로 보므로, 필드를 남기면 외부 요청이 사용자 요청으로 판정될 수 있다.

- **링크 클릭 라우팅** — 문서 안의 모든 non-anchor 링크 destination 은 HTML 생성 시점에 내부 nav-fragment 스킴(`#tasty-nav:link:<percent-encoded-dest>`)으로 rewrite 된다(`render::rewrite_link_dest`) — 실제 `href` 를 그대로 두면 native WebView 가 진짜 파일/미지 스킴으로 navigate 해버려(host 는 *원격* http(s) 만 차단) 렌더된 문서가 그 자리에서 깨진다.
  fragment 만 바뀌는 same-document navigation 은 (a) WebKitGTK 의 `decide-policy` 와 Windows 의 `HistoryChanged`([WebView §탐색](../../design/systems/webview.md))로 캡처되지만(→ host 가 `webview.navigation_attempt` 이벤트로 forward) (b) 실제 페이지 리로드는 일으키지 않는다(실측 검증됨) — 이 성질로 "클릭을 가로채되 화면은 안 깨지는" 신호 채널을 만든다.
  plugin 의 `on_webview_navigation_attempt` 핸들러가 그 이벤트를 받아 `render::parse_nav_fragment`+`classify_link` 로 판정한다:
  - **상대 경로**(`docs/index.md`, `../sibling.md`)는 **현재 마크다운 파일의 폴더(base_dir) 기준**으로 절대화(프로세스 cwd 아님). 절대 경로는 그대로. `javascript:` 는 무조건 무시(sanitize 와 별개의 2차 방어).
  - **외부 URL**(`http(s)://`·`mailto:`·`data:`)은 host `webview.open_external {surface_id,url}` 로 보내고, host 가 OS 기본 핸들러로 연다. **이 plugin 은 OS 열기를 직접 하지 않는다**(OS 열기 크레이트를 링크하지 않는다) — host 한 자리로 모아야 debug 스위치(`TASTY_DEBUG_OS_OPEN_LOG`, [유닛 테스트 격리 — 사용자 환경을 읽지 않는다](../../dev-guide/unit-test-isolation.md))가 이 열기도 기록한다. host 는 호출 plugin 이 `surface_id` 의 소유자이고 URL 에 스킴이 있으며 `javascript:` 가 아닐 때만 연다. 클릭하면 확인 없이 기본 브라우저가 열리는 사용자 동작은 그대로다(아래 기능 계약).
  - 파일 링크는 host `file_handler.dispatch` 로 보낸다(Explorer "파일 열기" 와 동일한 `DispatchFile` 경로 — 그 surface 가 속한 **Pane 의 새 탭**). 통지받은 navigation 의 URL 을 `user_navigation_url` 로 그대로 되대고(`file_link_params`), host 가 직접 본 두 사실(엔진의 사용자 제스처 보고 · 그 문서 페이지를 이 plugin 이 썼다)로 그 호출을 사용자 행동으로 판정하면 새 탭이 선택된다 — Linux 는 실측, Windows 는 컴파일만 재어졌다(재는 법: Windows 에서 문서 안의 파일 링크를 누르고 새 탭이 선택되는지 `tab.list` 로 본다). macOS 는 엔진 대신 WebView 가 받은 OS 클릭으로 제스처를 정한다([WebView 탐색](../../design/systems/webview.md#탐색-상태와-실행-소유)). macOS 실제 클릭 결과는 측정하지 않았다. plugin 은 판정하지 않는다([파일 핸들러 (File handler)](../../features/file-handler/index.md)).
- **주소창** — 문서 안의 HTML 띠로 만든다(`render/address.rs`, 디자인 갤러리 `markdown-address-bar`·`markdown-address-folder`·`markdown-address-failures`·`markdown-address-narrow`). 로컬 문서는 [Folder · 필드 · Go], mirror 문서는 [Folder · Reload · 필드 · Go]다.
  - 버튼은 IconButton ghost sm 과 같은 상자(`icon-button-size-sm`, 글리프 `icon-size-md`)이고 글리프는 CSS mask 라 hover·disabled 글자색을 따른다. 필드는 PathField 상자(`input-height`)에 kind 글리프(로컬 file, 원격 remote)와 고정폭 경로를 둔다. 포커스가 없을 때는 경로를 `md-address-value-fg` 로 그리고 앞쪽부터 말줄임해 파일 이름이 보인다. 치수·색은 모두 `md-address-*`·`input-*`·`icon-button-*` 토큰에서 읽는다.
  - **키보드** — 단축키 설정 `explorer_focus_address`(주소창으로 이동, 기본 `Alt+L`, macOS `⌘L`)를 plugin 이 `settings.get_keybinding` 으로 읽어 문서에 넣는다. 페이지 어디서 누르든 `#tasty-addr-input` 에 포커스를 주고 값 전체만 고른다. 이미 편집 중이어도 다시 고르며, 연결이 끊긴 읽기 전용 경로도 고르고 복사할 수 있다. host 는 markdown 페이지에서 이 키를 가져가지 않는다. 바인딩이나 `find` 를 바꾸면 `command.host_shortcut_changed` 로 문서를 다시 만들어 새 키만 동작한다. 다시 만들면 주소칸·검색칸에 쓰던 입력과 IME 조합은 사라진다. 스크롤 위치는 아래 스크롤 위치 보존(best-effort)을 따르며 복원을 약속하지 않는다. Enter·Go 는 같은 이동을 한 번 보내고(읽기 전용이면 보내지 않는다), Esc 는 로컬·원격 모두 입력을 지금 경로로 되돌리고 포커스를 놓으며 이동하지 않는다. IME 조합 중의 Enter·Esc 는 이동·취소로 쓰지 않는다. 주소칸의 Esc 는 열린 검색 바를 함께 닫지 않는다. 복사·잘라내기·붙여넣기·전체 선택과 실행 취소·단어 이동은 [WebView 편집 액션](../../design/systems/webview.md#편집-액션)이 포커스된 입력란(주소·검색)이나 본문에 실행한다. 주소 띠와 검색 바는 입력란만 고를 수 있어 본문 전체 선택·복사에 경로 표시나 버튼 글자가 섞이지 않는다. 검색 바를 닫으면 열기 전에 포커스가 있던 요소(주소칸 등)로 포커스를 돌려준다.
  - 로컬 필드에만 이 머신의 최근 경로를 native `<datalist>`(`tasty-addr-recent`)로 붙인다. 원격 경로 칸에 로컬 경로를 제안하면 안 되므로 mirror 문서에는 내지 않는다.
  - 페이지 폭에서 필드가 `md-address-go-hide-below` 아래로 줄어들 때 Go 만 숨긴다(media query, 경계 폭은 토큰으로 계산). Enter 는 그대로 이동하고 Folder·Reload 는 숨지 않는다. 필드는 `md-address-field-min-width` 아래로 줄지 않으며 더 좁으면 띠 끝이 잘린다.
  - **폴더 버튼**(`#tasty-browse`, 글리프 folderOpen, 툴팁 `markdown.address.open` / 원격 `markdown.address.open_remote`)은 `#tasty-nav:browse:<nonce>` 를 보낸다. plugin 은 팝업 없이 `file_picker.trigger {filters:["md","markdown"], start_dir, origin_surface_id, user_navigation_url}` 로 host 피커를 연다(`browse.rs`).
    - `user_navigation_url` 은 버튼 클릭이 낸 navigation URL 이다. host 가 파일 링크와 같은 규칙으로 사용자 조작을 확인하면 피커를 사용자 요청으로 열어 키보드 포커스를 준다. 이 피커는 일반 대상 팝업이라 WebView 는 오버레이 모드를 따른다. 불투명이면 페이지를 보인 채 피커 상자만 가리고, 숨김이면 페이지를 숨긴다([WebView §egui 오버레이와 native WebView](../../design/systems/webview.md#egui-오버레이와-native-webview)). 확인하지 못하면 포커스 없는 passive 피커가 되고, 방금 눌러 키를 받는 이 문서 페이지가 피커를 덮는다([내부 팝업 시스템](../../design/systems/popup.md#passive-팝업)). Linux 는 실측했고 Windows 는 컴파일만 재었다. macOS 는 버튼을 누른 OS 클릭으로 확인한다([WebView 탐색](../../design/systems/webview.md#탐색-상태와-실행-소유)). 입력 없이 스크립트가 낸 이동은 어느 OS에서도 사용자 조작으로 보지 않는다. 키보드로 누른 버튼(Enter·Space)은 Linux 엔진과 Windows host 입력 경로에서 사용자 조작이고 macOS에서는 아니다. macOS 27.2에서 마우스로 누른 버튼은 사용자 요청 피커로 열려 문서 위에 바로 보였고, 시작 폴더·필터·같은 surface 열기·취소를 실측했다.
    - 시작 폴더는 문서 폴더다. 로컬은 열린 파일의 부모 폴더, mirror 는 원격 경로의 부모(드라이브 문자나 `\\` 로 시작하면 `\`, 그 밖은 `/` 로 나눔)다. host 의 surface cwd 대신 명시하는 이유는 mirror surface 의 cwd 가 서버가 push 한 셸 cwd 일 수 있어 문서 폴더와 다를 수 있기 때문이다. 원격 폴더 목록과 존재 판정은 원격이 한다.
    - 요청 번호는 그 surface 에 묶어 둔다. 같은 surface 에서 새로 누르면 이전 요청의 결과는 버리고, surface 가 닫히면 대기 요청도 버린다. 취소·늦은 결과·모르는 번호는 아무것도 하지 않는다. 피커가 이미 열려 있으면(`-32000`) 두 번째 피커를 열지 않고 warn 로그만 남긴다. 열린 피커를 마치거나 취소한 뒤 다시 누르면 된다. 같은 surface에서 연 피커가 passive로 깔려 있으면 사용자 입력이 확인된 다음 누름이 그 피커에 포커스를 주고, host는 첫 요청 번호를 돌려준다.
    - 확정 결과가 오면 고른 경로 그대로 `markdown.navigate {surface_id, path, picker_request_id}` 를 보내 같은 surface 를 바꾼다. 새 탭을 만들지 않고 클립보드도 쓰지 않는다. host 는 그 경로를 이 plugin 의 피커에서 사용자가 확정했는지 확인해 사용자 요청으로 판정하므로([네이티브 파일 피커](../../features/native-file-picker/index.md)) mirror 변환이 원격에서 실패하면 toast 로 알린다. 주소창 직접 이동의 오류 행과 섞지 않는다. mirror 문서는 주소창 이동과 같이 `run.result` 를 기다리고(`markdown.address_result` 의 `browse: true`), 성공하면 원문을 다시 요청하고 실패는 오류 행으로 만들지 않는다. 결과를 기다리던 주소창 직접 이동을 이 이동이 대신했으면 문서를 한 번 다시 실어 그 이동의 30초 대기 타이머와 이미 보인 대기 행을 지운다. 대신한 직접 이동의 늦은 결과는 순번이 달라 버려진다. 피커를 취소하면 직접 이동은 그대로 결과를 기다린다. 로컬 파일이 그사이 사라졌으면 host 가 `-32602` 로 거절하고 plugin 은 warn 로그만 남긴다.
    - 연결이 끊긴 mirror 문서에서는 다른 버튼처럼 disabled 이고 늦게 온 신호도 무시한다.
  - 툴팁은 버튼을 감싼 요소의 `title`이다. disabled 버튼은 마우스 이벤트를 받지 않아 자기 `title`을 보이지 않기 때문이다. native 툴팁이라 위치는 OS가 정한다.
  - 문서 생성 시점의 *현재* 경로/최근목록으로 만들어지며, webview 에는 JS↔plugin 실시간 메시지 브리지가 없어(module doc 참고) 포커스 시 다시 fetch 하는 반응형 동작은 없다. Go 클릭/Enter 는 신뢰된 인라인 `<script>`(`render::nav_script`)가 `location.hash = 'tasty-nav:addr:<nonce>:' + encodeURIComponent(v)` 로 위 nav-fragment 채널에 태워 보낸다. nonce 가 있어 같은 경로를 다시 보내도 hash 가 바뀐다. `encodeURIComponent` 는 `:` 를 남기지 않으므로 plugin 은 첫 `:` 앞의 숫자를 nonce 로 떼어 낸다.
- **attach mirror 문서** — 원격 tasty 의 워크스페이스를 attach 하면 그 안의 markdown surface 는 이 plugin 이 **원격에서 가져온 원문**으로 다시 그린다([원격 attach (Remote attach)](../../features/remote-attach/index.md)).
  host 가 surface 를 만들 때 params 의 `remote.file` 에 원격 경로를 싣고(최상위 `file` 이 아니다), plugin 은 그것을 보면 `MdDoc::new_remote` 로 문서를 연다 — **파일을 읽지 않고, 감시에 등록하지 않고, `base_dir` 를 두지 않고, snapshot 을 내지 않는다.** 원문은 `markdown_mirror.content_request {surface_id}` 로 요청해(바깥 호출자의 `markdown.reload` 가 건 요청만 `agent_origin: true` 를 더 싣는다 — host 가 그 회신의 잘림 toast 를 사용자에게 띄우지 않는다, [내부 팝업 시스템](../../design/systems/popup.md)) `request_id` 를 받아 두고, host 가 unicast 하는 `markdown_mirror.content_result` 이벤트 중 **그 id 의 것만** 반영한다(새로고침을 연달아 누르면 옛 회신은 버려진다). 성공 회신의 `file` 은 원격 surface 가 지금 보여 주는 경로다. 원격이 같은 surface 를 다른 파일로 바꾸면 클라이언트의 mirror surface 는 그대로 남으므로, plugin 은 이 값으로 주소창 경로를 바꾼다.
  회신의 `reason` 은 `load_error` 로 가 실패 상태로 그려지고, 원문을 한 번도 못 받은 동안은 로딩 상태(`markdown.remote.loading`)가 그려진다.
  `request_id: 0`은 연결 끊김을 알리는 신호다.
  대기 중인 요청을 끝내고, 이미 원문을 받은 문서도 끊김 화면으로 바꾼다.
  재연결 뒤 변경 신호를 받으면 원문을 다시 요청한다.
  - **새로고침**: 필드 왼쪽의 Reload(`#tasty-refresh`)는 `#tasty-nav:refresh:<nonce>` 로 nav-fragment 채널을 타고, plugin 은 그것을 원문 재요청으로 옮긴다(nonce 는 같은 hash 재대입이 navigation 을 안 일으켜 두 번째 클릭이 사라지는 것을 막는다). `markdown.reload` 도 mirror 문서에는 같은 재요청이다.
  - **주소창 이동**: 필드는 편집할 수 있고 Enter·Go 가 같은 surface 를 그 원격 경로의 문서로 바꾼다. plugin 은 `markdown.navigate {surface_id, path}` 를 보내고 host 는 로컬 존재 확인 없이 원격에 변환을 전달한다. 원격이 그 경로에 파일이 없으면 변환하지 않고 거절한다(아래 실패 표시). Esc 는 입력을 지금 원격 경로로 되돌린다. 입력은 그대로 보내며 상대경로는 원격 쪽 해석에 맡긴다.
  - **주소창 이동 실패**: 이 요청에는 사용자 출처 증명을 싣지 않아 host 는 실패 toast 를 띄우지 않는다. 대신 plugin 이 접수 응답의 `run` 을 별도 스레드에서 `run.result` 로 기다렸다가 `markdown.address_result` 자기 호출로 결과를 워커에 돌려준다(외부 호출자는 이 메서드를 쓸 수 없다). 묻는 간격은 200ms 에서 두 배씩 늘려 3초에서 멈추고, 30초가 지나면 늦은 결과를 받으려고 시간 상한 없이 60초마다 묻는다. run 이 끝나면(보낸 뒤 연결이 끝나면 host 가 `unknown` 으로 끝낸다) 그치고, 진행 중이라는 답을 받은 뒤 host 가 그 run 을 더 모르게 되면(오래된 run 정리) 결과를 알 수 없어 기록만 남기고 그친다. host 의 slow_loop 탐지가 같은 run 을 5분에 20번 물으면 사용자에게 경고하므로 어느 5분 구간에서도 그 수에 닿지 않게 한다. 결과는 같은 surface 의 마지막 이동일 때만 반영한다.
    - 실패하면 입력한 경로를 필드에 남기고 오류 테두리와 띠 아래 오류 행(`role="alert"`)을 그린다. 행 모양은 하나이고 실패 종류마다 문장이 다르다. 문장 안의 경로는 고정폭이며, 행이 넘치면 끝을 줄이고 툴팁에 전체 문장을 둔다. 오류 행만큼 `--md-addr-bar-h` 도 늘려 검색 바와 앵커 여백이 따라간다.
    - 종류는 `run.result` 의 상태와 필드로만 정하고 결과 문장을 해석하지 않는다.

      | `run.result` | 종류 | 문구 키 |
      |---|---|---|
      | `failed` + `reason_code: "not_found"` | 원격에 파일 없음 | `markdown.address.error.not_found` |
      | `failed` + `remote_reason` | 원격 거절. 사유 원문을 번역하지 않고 `lang="en"` 으로 붙인다 | `markdown.address.error.rejected` |
      | `unknown` | 결과 전에 연결 끊김. 적용 여부를 말하지 않는다 | `markdown.address.error.connection_lost` |
      | `cancelled` | 보내기 전에 연결·대응이 바뀌어 취소 | `markdown.address.error.cancelled` |
      | 그 밖의 `failed`(사유 없음·보내기 전 host 실패), `run.result` 조회 실패, `markdown.navigate` 거절 | 요청이 원격에 닿지 않은 내부 실패 | `markdown.address.error.internal` |

    - 30초 안에 결과로 문서가 다시 실리지 않으면 페이지가 숨겨 둔 대기 행(`role="status"`, 공용 Spinner 모양 + `markdown.address.waiting`, 글자 `md-address-pending-fg`)을 오류 행 자리에 보이고 필드를 편집 모양으로 둔다. 페이지 안에서 행을 보일 뿐 문서를 다시 싣지 않으므로 입력 포커스와 IME 조합을 건드리지 않는다. 대기 중에 다른 이유로 문서를 다시 그리면 plugin 이 입력값과 남은 시간(`data-wait-ms`)을 넘겨 대기를 잇는다. 대기 행은 오류가 아니며 원격이 늦게 답하면 그 결과를 그대로 반영한다.
    - 오류 행과 대기 행은 다음 입력·Esc 에서 페이지가 지우고 `#tasty-nav:addr-dismiss:<nonce>` 로 plugin 에 알려 보관한 실패와 대기 상태도 버린다. 순번은 그대로라 그 뒤의 늦은 성공도 반영된다. 늦은 실패는 오류 행을 되살리거나 문서를 다시 싣지 않는다. 다시 실으면 그사이 쓰던 입력과 IME 조합을 이전 경로로 덮기 때문이며, plugin 은 info 로그만 남긴다. 이동에 성공하면 보관한 실패를 지우고 원문을 다시 요청해, 회신의 `file` 과 원문으로 새 문서를 그린다. 새 이동은 앞선 실패를 대신하며 결과가 앞선 실패와 같아도 다시 그린다.
  - **링크**: 파일 링크는 원격 호스트의 경로라 이 머신에서 열지 않고, 외부 URL 만 연다.
  - **연결 끊김**: host 가 끊김을 알리면(`request_id: 0`) 필드는 읽기 전용 상자(`input-readonly-*`, 경로 `state-disabled-fg`)가 되고 Folder·Reload·Go 는 모두 disabled, 각 툴팁은 `markdown.address.disconnected` 다. 주소창 이동이 결과 전에 끊겨 실패했으면 그 오류 행(`error.connection_lost`)은 끊긴 동안에도 남고, 대기 행은 보이지 않는다. 재연결 뒤 원문을 다시 받으면 풀린다.
  - **변경 신호**: `markdown_mirror.changed {surface_id}` 이벤트를 받으면 원문을 다시 받지 않고 Reload 의 `data-stale` 만 `true` 로 바꿔 `md-address-stale-bg`·`md-address-stale-fg` 로 채운다(띠에서 유일하게 채운 요소, 툴팁 `markdown.address.stale`). 최신 원문을 받으면 풀린다. 이미 stale 이면 문서를 다시 싣지 않는다.
  - **테마 변경**은 받아 둔 원문으로 다시 그릴 뿐 요청하지 않는다. mirror 문서는 host 의 파일 핸들러를 거치지 않으므로 최근목록에도 기록되지 않는다.
- **최근목록 조회** — host 의 generic **`recent.query {kind}`** IPC 가 그 kind 의 최근 연 파일을 **최신순 최대 10개** 반환한다(`{recent:[{path,file_name}]}`). markdown plugin 은 `kind:"markdown"` 을 채워 호출해 위 주소창 `<datalist>` 를 채운다. **읽기 전용** — host의 모든 창이 공유하는 `MainViewState.recent_files` 캐시를 필터 없이 조회할 뿐 사용자 상태를 바꾸지 않는다([포커스 정책](../../design/policies/focus.md)). **`surface.read`** 권한. recent 기록 대상 여부는 매니페스트 `records_recent` capability 로 판정. mirror surface 를 변환하거나 mirror pane 에 새 탭을 열 때는 경로가 원격 파일이라 기록하지 않는다. 새 탭은 탭이 만들어질 pane 이 mirror workspace 에 속하는지로 판정하며, 구조 변경을 원격에 forward 하는 판정과 같은 기준이다.
- **스크롤 위치 보존(best-effort)** — idle-watch 자동 리로드와 `markdown.reload` 는 전체 문서를 `load_html` 로 통째로 교체한다(부분 DOM patch 없음) — 이대로면 native WebView 의 스크롤이 매 리로드마다 0 으로 리셋된다. `render::nav_script` 가 `sessionStorage`(파일 경로로 키잉, `scroll` 이벤트 150ms 디바운스 저장 + 로드시 복원)로 이를 완화한다. same-origin 스코프 의존이라 `load_html` 반복 호출이 origin identity 를 유지하는지는 webview 엔진(WebKitGTK/WKWebView/WebView2)마다 다를 수 있어 3 종 전체 검증되지 않았다 — 실패하면 스크롤 위치가 복원되지 않으며 별도 오류는 표시하지 않는다. Linux(WebKitGTK)에서는 바인딩 변경과 내용이 바뀐 새로고침으로 다시 만든 문서가 맨 위에서 시작하는 것을 확인했다.
- **cli** — `tasty markdown recent`(최근목록 조회 — plugin CLI 서브커맨드가 `recent.query` 로 trampoline). reload 등도 plugin CLI.
- **settings_page** — `markdown` 페이지.

## 인터페이스

- **사용자**: 마크다운 파일 열기 → markdown surface, 또는 surface 종류 전환.
- **AI Agent**: `tasty markdown …` CLI / `markdown.*` IPC. surface 생성은 [work-area](../../features/work-area/index.md) (`--type markdown`).

## 비-목표

- surface 배치/생성 도메인 — [work-area](../../features/work-area/index.md).
- 픽셀/타이포 — design-system.

## Acceptance Criteria

- Given markdown 플러그인 활성 When 마크다운 파일 열기 Then markdown surface 로 렌더된다.
- Given `tasty new tab --type markdown --file <f>` Then 그 파일이 렌더된다.
- Given `tasty list surfaces` Then 해당 surface 가 `kind:"markdown"` 으로 보고된다.
- Given 대용량 확인을 취소한 markdown surface When 그 파일이 바뀐다 Then 감시가 파일을 읽지 않는다. When `tasty markdown reload --surface <ID>`를 부른다 Then 확인 팝업이 다시 뜨고 응답의 `deferred`가 `true`다.
- Given 대용량 확인 팝업 When 취소·Esc·바깥 클릭으로 닫는다 Then 본문 자리에 "이 파일은 비어 있습니다"가 아니라 `markdown.state.large_deferred`(1 MiB를 넘는 파일이라 아직 읽지 않았다는 문구)가 보인다.
- Given 대용량 확인을 취소해 대기 상태인 markdown surface When 본문의 "Open file…" 버튼을 누른다 Then 파일을 바로 읽지 않고 같은 대용량 확인 팝업이 다시 뜬다. When 다시 취소한다 Then 같은 대기 상태와 버튼이 남는다.
- Given attach mirror markdown 문서 When 1 MiB를 넘는 원문을 받는다 Then 대용량 확인 없이 표시하므로 대기 상태와 "Open file…" 버튼이 나오지 않는다.
- Given 1 MiB 이하로 읽어 둔 markdown 문서 When 파일이 1 MiB를 넘게 커진다 Then 내용을 다시 읽지 않고 대용량 확인 팝업을 띄우며, 팝업이 떠 있는 동안 이전 내용이 보인다. When 취소한다 Then 이전 내용이 사라지고 대기 상태가 된다. When [열기]를 고른다 Then 새 내용이 보인다.
- Given markdown surface가 아닌 ID When `tasty markdown reload --surface <ID>` Then invalid params 오류로 끝나고 성공으로 답하지 않는다.
- Given 로컬 파일을 연 markdown surface When 그 WebView의 web process가 끝난다 Then 다음 동기화에서 같은 본문을 다시 실어 문서가 다시 보인다([WebView](../../design/systems/webview.md#탐색-상태와-실행-소유)).

## 화면

- [screens/markdown.md](screens/viewer.md) — 마크다운 렌더 surface.
