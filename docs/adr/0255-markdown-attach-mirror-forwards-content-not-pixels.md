# ADR-0255: attach mirror 의 markdown surface 는 픽셀이 아니라 **원문**을 나른다 — 새 role 과 lazy 조회 채널

- **Status**: Accepted
- **Date**: 2026-09-09
- **Tags**: markdown, attach, mirror, remote, wire-format, webview, surface-role, lazy-fetch, budget, occupancy-trust, adr-0053, adr-0056, adr-0059, adr-0065
- **Group**: remote-attach

## Context

workspace attach 로 mirror 한 워크스페이스에서 markdown surface 는 **빈 placeholder** 로만 뜬다.
서버측 `src/core/attach_runtime.rs::build_workspace_tree_surfaces` 가 markdown 을
`AttachSurfaceClass::non_terminals`(또는 mesh 화이트리스트 탈락분)로 보고
`{"remote_id", "role":"placeholder", "kind"}` 만 내려보내고, client 의
`src/app/attach_client.rs::build_layout` 은 term/mesh/explorer 어느 맵에도 없는 leaf 를
`EmptySurface` 로 만든다.

과거에는 markdown 이 egui-mesh mirror 대상이었다. [ADR-0065](0065-markdown-webview-render-channel.md)
로 `rendering = "webview"` 가 되면서 `src/core/surface_registry/egui_mesh.rs::is_egui_mesh_allowed`
의 화이트리스트(`("image","com.tasty.image") | ("mesh_demo","com.tasty.mesh-demo")`)에서 빠졌다 —
webview kind 에는 plugin 이 egui-mesh 프레임을 그릴 채널 자체가 없다
(`src/adapters/ui/surface/webview_chrome.rs` 는 host 소유 fallback chrome 일 뿐이다).

이 빈칸은 채우기로 이미 방향이 잡힌 자리다. [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md)
의 "화면 동기화" 앞 절이 **"placeholder 로 남는 비-터미널 surface 의 mirror 불가는 기술적
미구현이지 보안상 의도적 배제가 아니다"** 라고 못박고 있다.

**"내용 전달" 이 성립하는 이유**: markdown plugin 의 렌더 입력이 이미 파일이 아니라 **문자열**이다.
`crates/tasty-plugin-markdown/src/render.rs::render_document` 는
`DocumentInput { theme, tr, file_path, source, load_error, base_dir, recent }` 를 받고 `source: &str`
만 있으면 문서를 만든다. 파일 read 는 `MdDoc::new`/`force_reload` 가 별도로 한다. 즉 client 에
markdown plugin 이 있으면, **원문 문자열만 건네받아 client 가 자기 테마·자기 recent 로 렌더**할 수 있다.

**선례**: 원격 데이터를 client 가 받아 client 가 그리는 채널이 이미 셋이다 —
File Picker([ADR-0053](0053-native-file-picker-remote-attach-channel.md), `list_dir_request`/
`list_dir_result`) · git-viewer([ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md),
`git_query_request`/`git_query_result`) · explorer([ADR-0059](0059-explorer-remote-attach-list-dir-reuse-browse-only.md),
list_dir 채널 재사용). 셋 다 `StreamControl` enum **밖**의 raw JSON `event` 태그를 같은
`StreamTag::Control` 채널에 싣고, 인가는 `CoreState::attach.client_holds_workspace(client_id)`
("attach 점유 = 신뢰") 하나만 본다. explorer 는 그 위에 전용 role(`"explorer"`)까지 두었다.

**모델 크레이트의 제약**: markdown surface 는 본체 크레이트의 `RemoteSurface`
(`src/plugin_bridge/remote_surface.rs`)라 `tasty-model` 이 downcast 로 식별할 수 없다 —
explorer 가 쓴 `downcast_ref::<ExplorerPanel>()` 수법을 쓸 수 없는 자리다. 같은 제약을 이미
`Surface::attach_mesh_info()`(`crates/tasty-model/src/surface_trait.rs`, 기본 구현 `None`,
`EguiMeshSurface` 만 override)가 "trait 는 하위 crate 에, 구현은 상위 crate 에" 로 풀었다.

**경로는 서버가 이미 안다**: markdown plugin 이 `open_file_surface` 에서
`snapshot: file.map(|f| json!({"file": f}))` 를 올리고, host 가 그것을
`RemoteSurface.snapshot_cache` 에 캐시한다.

## Decision

markdown mirror 는 **렌더 결과(픽셀·HTML)가 아니라 원문 문자열**을 나른다. 핸드셰이크는 좌표만
싣고, 내용은 전용 이벤트 쌍으로 **lazy 하게** 가져온다. 아래 일곱 항목의 값을 확정한다.

### 1. 전용 role — `"markdown"`

placeholder 를 확장하지 않고 explorer 선례대로 전용 role 을 둔다. 서버가 싣는 per-surface
디스크립터는

```
{"remote_id": <원격 surface id>, "role": "markdown", "file": "<원격 절대경로>", "display_name": "<탭 제목>"}
```

- `file` 은 **표시·제목 전용의 opaque 문자열**이다. client 는 이 값으로 자기 로컬 파일을 열지
  않는다 — 두 인스턴스의 파일시스템은 다르고, 같은 경로에 다른 파일이 있으면 조용히 남의 문서를
  그리게 된다. 파일이 없는 markdown surface(빈 문서)면 빈 문자열이다.
- **내용은 핸드셰이크에 싣지 않는다.** 트리 디스크립터가 문서 크기만큼 부풀면 attach 성립 자체가
  프레임 상한에 걸린다. 내용은 항목 2 의 채널로 따로 가져온다.
- 판정 범위는 **markdown 하나로 좁힌다.** 분류는 model 이 후보만 모으고(항목 1a), 최종 허용은
  앱 계층이 `("markdown", "com.tasty.markdown")` 화이트리스트로 재검증한다 — mesh 가
  `is_egui_mesh_allowed` 로 하는 것과 같은 두 단이다. 이 좁힘이 없으면 같은 webview kind 인
  html surface(`crates/tasty-plugin-html`)까지 새 role 로 나가는데, html 의 URL 은 파일 경로라는
  보장이 없어 "그 경로의 원문" 이라는 이 채널의 의미가 성립하지 않는다.

**1a. 분류 경로** — `Surface` trait 에 `attach_mesh_info` 와 동형의 `attach_content_info()` 를
추가한다(기본 구현 `None`). 답은 `(kind, plugin_id, file: Option<...>)` 세 값이고, `RemoteSurface`
가 override 해 `kind_static`·`plugin_id` 와 `snapshot_cache` 의 `"file"` 키를 답한다.
`AttachSurfaceClass` 에 대응 버킷을 추가하고, `classify_attach_surfaces` 의 분기 순서는
**mesh → content → explorer → terminal/non_terminal** 로 한다 — mesh 가 먼저여야 기존
image/mesh_demo 분류가 안 바뀐다.

### 2. 내용 조회 채널 — `markdown_content_request` / `markdown_content_result`

`list_dir`/`git_query` 와 동일한 형태(같은 `StreamTag::Control`, `StreamControl` enum 밖의 raw
JSON `event` 태그, 알 수 없는 event 는 조용히 무시)다.

client → server:

```
{"event": "markdown_content_request", "request_id": <u64>, "surface_id": <원격 surface id>}
```

server → client, 성공:

```
{"event": "markdown_content_result", "request_id": <u64>, "surface_id": <원격 id>,
 "ok": true, "file": "<원격 경로>", "source": "<원문>", "truncated": <bool>}
```

server → client, 실패:

```
{"event": "markdown_content_result", "request_id": <u64>, "surface_id": <원격 id>,
 "ok": false, "reason": "<사유>"}
```

- **에러 채널은 하나다.** 티켓 초안이 `load_error` 필드를 따로 두는 형태를 스케치했으나, 그러면
  같은 사실(읽지 못했다)이 `ok:false`/`reason` 과 `load_error` 두 자리에 적히고 한쪽만 채워지는
  상태가 표현 가능해진다. 채널은 `list_dir_result`/`git_query_result` 와 똑같이 `ok` + `reason`
  하나로 두고, **client 가 그 `reason` 을 `DocumentInput.load_error` 로 옮겨** 렌더한다 — wire 의
  실패와 렌더의 실패 표시는 층이 다르고, 변환은 그리는 쪽이 한다.
- **"파일 없는 surface" 는 에러가 아니다.** 원격 markdown surface 가 파일 없이 열려 있으면
  서버에서도 빈 문서가 보이므로, 그 상태의 충실한 mirror 는 `ok:true` + `file:""` + `source:""` 다.
  `ok:false` 로 답하면 mirror 가 서버에 없는 에러를 만들어낸다.
- `surface_id` 를 회신에도 싣는다 — client 는 mirror surface 별로 자기 pending 을 추적하고
  (explorer 가 `ExplorerViewStore` 에 pending 을 두는 것과 같은 방향: host 에 범용
  `request_id → consumer` 레지스트리를 만들지 않는다), 응답을 그 surface 로 되돌린다.
- **인가는 새로 만들지 않는다.** `client_holds_workspace(client_id)` 하나만 본다
  (ADR-0053/0056/0059 과 동일). 판정 기준은 "SSH 로 이미 가능한가" 이고, 이 채널이 나르는 것은
  그 원격 호스트의 파일 원문 — SSH 로 붙은 사용자가 이미 읽을 수 있는 것이다. 새 permission
  토큰을 만들지 않는 것은 [`docs/dev-guide/plugin-permissions.md`](../dev-guide/plugin-permissions.md)
  가 mesh mirror 때 이미 내린 같은 판단이다.
- **그 술어가 인가하는 집합은 engine 전체다 — 그 surface 의 워크스페이스가 아니다.** 여기서
  갈라 적는 이유는 이름이 오해를 부르기 때문이다: `client_holds_workspace` 는 **어떤**
  워크스페이스든 하나 점유했는가만 보고(`src/core/attach.rs`, `workspace_locks` 전체 스캔),
  대상 조회 `markdown_content_for_request` 는 `find_surface_by_id`
  (`src/core/state/finders.rs`, **전 워크스페이스 순회**)로 surface 를 찾는다. 그래서 W2 만
  점유한 client 가 W1 의 markdown surface 원문을 받는다. 이것은 이 채널의 일탈이 아니라 형제
  셋의 공통 형태다 — list_dir(ADR-0053/0059)은 wire 에 `dir` 문자열만 실어 워크스페이스 바인딩
  필드가 **아예 없고**, git_query(ADR-0056)는 engine 전역 `TerminalStore` 에서 `surface_id` 를
  찾거나(`resolve_git_query_target`) `worktree_path` 로 임의 경로를 그대로 받으며, 캡처
  업로드(03, `finalize_capture_upload`)는 workspace 를 보지도 않는다. attach 의 인가는 두 갈래인데
  (입력·resize·구조 op forward 는 anchor 워크스페이스의 holder 를 `workspace_holder(ws) ==
  client` 로 직접 검증하고, 콘텐츠 조회 채널은 `client_holds_workspace` 하나만 본다), 이 채널은
  **뒤쪽**에 속한다. 넓은 쪽이 안전한 이유는 위 판정 기준 그대로다 — SSH 로 붙어 attach 를
  성립시킨 사용자는 이 호스트의 그 파일들을 이미 읽을 수 있고, 좁혀도 같은 사용자가 W1 에
  다시 붙으면 같은 것을 얻는다(좁힘은 경계가 아니라 왕복 한 번이다).
  **이 집합은 값으로 고정돼 있다** —
  `markdown_content_request_is_authorized_engine_wide_not_per_workspace`
  (`tests/attach_markdown_content_loopback.rs`)가 W1(markdown surface)·W2 를 만들고 **W2 만
  점유한** client 로 W1 의 원문을 요청해 `ok:true` 와 원문을 받는다. 인가를 그 surface 의
  워크스페이스 holder 로 좁히면(`workspace_holder_of(surface_id) == Some(client_id)`) 그
  test 만 빨개진다 — 나머지 넷은 좁힌 인가로도 통과하므로 이 축을 재는 것은 그 test 뿐이다.

### 3. payload 예산 — 직렬화 700 KiB, 문자 경계에서 자르고 `truncated` 로 알린다

`MARKDOWN_CONTENT_BYTE_BUDGET = 700 * 1024`. 근거는 `LIST_DIR_ENTRIES_BYTE_BUDGET` ·
`GIT_QUERY_BYTE_BUDGET` 과 **같다** — attach 프레임 하드 상한(`crate::ipc::stream::MAX_FRAME_LEN`,
1 MiB)보다 충분히 작게 잡아 envelope 오버헤드와 serde_json 이스케이프 팽창분을 흡수한다. 이
상한이 없으면 큰 문서 하나가 `write_frame` 을 `MAX_FRAME_LEN` 초과로 실패시키고, 그 세션의 write
thread 가 통째로 죽어 mirror 연결 자체가 끊긴다.

**재는 대상은 원문 바이트가 아니라 JSON 문자열이 된 뒤의 바이트다**(감싸는 따옴표 두 개 포함).
두 형제 예산이 `serde_json::to_vec(&wire).len()` 으로 재는 것과 같은 자리를 잰다 — 상한을 지켜야
하는 것은 프레임이고 프레임에 실리는 것은 이스케이프된 형태이기 때문이다. 이 구분이 markdown
에서는 형제들보다 크게 벌어진다: 형제들이 나르는 것은 경로·해시 같은 짧은 필드지만 여기서는
문서 전체가 한 문자열이라, 이스케이프 팽창분이 그대로 프레임 크기가 된다. 팽창률은 `"`·`\`·
개행에서 2 배, 그 밖의 제어문자에서 6 배(`\u00XX`)까지 간다 — 원문 700 KiB 를 통과시키면 따옴표가
절반쯤인 문서(코드블록에 JSON 을 담은 문서 등) 하나로 프레임이 1 MiB 를 넘어, **이 예산이 막으려던
바로 그 연결 끊김이 난다.** 그래서 예산이 보장하는 것은 "원문 700 KiB 까지 온다" 가 아니라
"프레임은 절대 안 넘는다" 이고, 실제로 실리는 원문은 이스케이프가 많을수록 짧아진다(최악 약
116 KiB). 그 사실은 `truncated` 가 알린다.

자를 때는 **UTF-8 문자 경계에서** 자른다(바이트 중간에서 자르면 문자열이 아니게 된다) — 비용을
`char` 단위로 누적하므로 경계는 구조적으로 보장된다. 잘렸으면 `truncated: true` 로 알린다 —
코드펜스·표 중간에서 잘려 렌더가 깨질 수 있다는 사실은 플래그 하나로만 전달하고, 어디서
잘렸는지는 구분하지 않는다(git_query 의 단일 `truncated` 와 같다).

**client 는 `truncated: true` 를 toast 로 알린다** — 문서 본문에 "여기서 잘렸다" 를 심지 않는다.
같은 플래그를 먼저 쓴 `list_dir` 이 그렇게 정했고(File Picker 의 `filepicker.remote_listing_truncated`,
[ADR-0059](0059-explorer-remote-attach-list-dir-reuse-browse-only.md) 결정 7), 이유도 그대로다:
본문에 심으면 그것이 원문의 일부인지 tasty 가 넣은 말인지 구분되지 않고, markdown 에서는 심는
자리가 코드펜스 안일 수도 있어 더 나쁘다. 이 자리를 값으로 닫아 두는 것은 client 를 짓는 후속
lane 이 재량으로 정하지 않게 하기 위함이다 — 같은 사실을 두 UI 가 다르게 표시하면 그 갈림은
사용자에게만 보인다.

이스케이프 비용표(`json_escaped_char_len`)는 serde_json 의 규칙을 손으로 옮긴 것이라 어긋나면
예산이 조용히 빗나간다 — BMP 전수 대조 테스트(`escaped_char_len_matches_serde_json`)가 그
어긋남을 잡는다. 재구현 대신 `serde_json::to_vec` 을 접두사마다 부르는 방법도 있으나, 그것은
문서 하나에 이분 탐색 20 회분의 재직렬화를 요구한다 — 한 번 걷는 쪽을 골랐다.

이 예산은 markdown plugin 의 대용량 게이트(`LARGE_FILE_LIMIT_BYTES = 1 MiB`, 초과 시
`pending_large` 로 read 를 보류하고 확인 팝업)와 **다른 층**이다 — 그쪽은 plugin in-process 의
사용자 확인이고, 이쪽은 wire 예산이다. 다만 값의 대소가 우연이 아니게 되도록 예산을 게이트보다
**작게** 둔다: 그 게이트를 건드릴 만큼 큰 파일은 직렬화하면 더 커지므로 예산에도 반드시 걸려
항상 `truncated: true` 와 함께 도착한다.

### 4. 파일을 읽는 주체 — 서버 host 가 직접 읽는다

`handle_git_query_request` 가 `tasty-git-core` 를 host 에서 직접 부르는 것과 같은 형태다. 서버측
markdown plugin 에 되묻지 않는다 — attach 요청 핸들러는 동기 경로이고 plugin 왕복은 비동기라 그
안에서 기다릴 수 없다. 읽기는 바이트로 하고 예산 안에서 자른 뒤 lossy 로 문자열화한다(비-UTF8
경로·내용의 lossy 변환은 `fs_list` 가 이미 쓰는 기존 처리다). 에러는
`list_dir_for_request` 와 같은 수준으로 구분한다 — `PermissionDenied` 는 `"permission denied"`,
그 외 io 에러는 `e.to_string()`.

**부작용을 명시한다**: 서버측 plugin 이 대용량 확인 대기(`pending_large`) 중이라 **서버 화면에는
아무것도 안 띄운** 파일이라도, host 는 예산 안에서 읽어 보낸다. 그 게이트는 "이 큰 파일을 지금
렌더할까" 라는 그리는 쪽의 물음이지 읽기 권한의 경계가 아니고, 이 채널의 경계는 항목 2 의
"attach 점유 = 신뢰" 하나다. 항목 3 의 예산이 게이트보다 작으므로 그 파일은 어차피 잘려서 간다.

### 5. 변경 신호 — 서버가 push 하고, client 는 다시 받지 않는다

client 는 **폴링하지 않는다.** 서버가 `{"event": "markdown_changed", "surface_id": <원격 id>}` 를
push 하고, client 는 그것으로 **refresh affordance 의 색만 바꾼다** — 자동으로 다시 받지 않는다.
사용자가 누를 때 항목 2 의 채널로 다시 가져온다. 자동 재수신을 하지 않는 이유는 client 가 보고 있는
문서가 사용자의 스크롤 위치를 가진 화면이고, 원격 편집이 잦을 때 그것을 말없이 갈아치우면 읽던
자리를 잃기 때문이다.

**신호의 발생원은 host 자신이다 — plugin 을 고치지 않는다.** markdown plugin 의 idle 감시
(`crates/tasty-plugin-sdk/src/file_watch.rs`, `RELOAD_CHECK_INTERVAL_SECS = 1.0`, content digest)는
변경을 감지하면 `HostHandle::self_invoke` 로 자기 `markdown.reload` 를 부르고 host 를 거치지
않는다. 그래서 "감시가 무엇을 봤는가" 는 host 에 안 온다. 그러나 그 reload 의 **결과**는 온다 —
`reload_webview` 가 항상 `push_html` → `webview.set_url` IPC 로 host 에 도달하고
(`src/adapters/ipc/handler/webview.rs`), host 는 그 surface 가 markdown kind 의 `RemoteSurface`
라는 것을 그 자리에서 안다. 그러므로 신호원은 **`webview.set_url` 을 받은 markdown kind
surface** 로 정한다. **신호원에는** 새 plugin→host 메서드도, SDK 변경도 필요 없다 —
이 문장은 신호 축에만 걸린다. 내용을 *가져오는* 축은 다르다: 항목 2 의 client↔server 다리
밖에 plugin↔host 다리가 하나 더 있고, 그쪽에는 새 메서드가 생긴다(항목 8).

**수신자는 새로 설계하지 않는다 — 항목 2 의 인가 술어를 그대로 뒤집어 쓴다.**
`client_holds_workspace` 가 참인 client(들), 즉 **이 engine 의 워크스페이스를 하나라도 hard 점유한
client 전부**에 push 한다. 항목 2 가 적은 대로 그것은 engine 전체이고, 그 surface 를 담은
워크스페이스의 holder 로 좁힌 집합이 **아니다** — 좁히면 요청은 통과하는데 신호는 안 오는 client 가
생긴다.

별도 구독 표를 두지 않는 이유가 그것이다: 표를 두면 "요청할 수 있는 client" 와 "신호를 받는 client"
가 갈라질 수 있는 상태가 표현 가능해지는데, 두 집합이 같아야 한다는 것이 이 채널의 성질이다. 그
등식은 **양쪽 다 engine 전체**로 성립시킨다. 반대로 좁혀서 맞추는 길 — 항목 2 의 조회를
`workspace_holder_of(surface_id)` 로 제한하는 것 — 도 등식은 만족하지만, 형제 채널 셋(list_dir ·
git_query · 캡처)이 전부 넓은 쪽이라 이 채널만 규칙이 달라지고, SSH 로 이미 가능한 것을 왕복 한
번으로 미룰 뿐이다(항목 2). 두 방향의 대가도 대칭이 아니다 — 넓은 쪽의 대가는 자기가 mirror 하지
않는 문서의 신호를 받은 client 가 물들일 affordance 를 못 찾아 그냥 버리는 것이고, 좁은 쪽의
대가는 사용자가 바뀐 줄 모른 채 낡은 화면을 계속 보는 것이다. 아래 "상위 집합" 논거가 이 비대칭에도
그대로 적용된다.

점유가 없으면 보낼 곳이 없으므로 신호는 그냥 사라진다 — 다음 attach 의 핸드셰이크가 최신 상태를
싣고 오므로 놓친 신호를 쌓아 둘 이유가 없다.

이 신호는 실제 파일 변경의 **상위 집합**이다 — 테마 변경·제자리 이동(`markdown.navigate`)·최초
생성도 같은 IPC 를 낸다. 그래도 무해하다: 신호가 하는 일이 affordance 를 물들이는 것뿐이라,
과잉 신호의 대가는 "눌러 보니 같은 내용" 이지 잘못된 화면이 아니다. 반대 방향(놓침)이었다면
사용자가 바뀐 줄 모른 채로 남으므로, 이 비대칭에서 상위 집합 쪽이 옳다.

### 6. client 에 markdown plugin 이 없을 때 — 현행대로 `EmptySurface`

판정은 `CoreState.surface_registry` 에 `"markdown"` kind 가 등록돼 있는가
(`SurfaceKindRegistry::contains`)다. 서버가 role 을 보내도 client 가 그릴 수 없으면 지금과 똑같이
빈 surface 로 떨어진다 — 서버는 client 의 plugin 구성을 모르고, 알 필요도 없다.

### 7. 스코프 밖 — 세 가지

[ADR-0059](0059-explorer-remote-attach-list-dir-reuse-browse-only.md) 가 파일 내용 fetch 를 명시적으로
제외한 것과 같은 형태로, 이번에 하지 않는 것을 이름으로 적는다.

- **상대경로 이미지·링크 대상**: client 에 그 파일이 없다. `DocumentInput.base_dir` 을 **`None`**
  으로 둔다 — 그래야 [ADR-0249](0249-markdown-local-images-are-inlined-by-the-renderer.md) 의
  이미지 인라이너가 기준 디렉토리 없이 상대경로를 해석하지 못하고 건너뛴다. base_dir 을 채우면
  **client 로컬의 같은 상대경로 파일**을 원격 문서의 그림으로 싣게 되는데, 그것은 깨진 그림보다
  나쁘다(틀린 그림이 맞는 것처럼 보인다). 깨진 참조는 그대로 노출한다 — 대체 문구를 넣으려면
  "원격이라 없음" 과 "원래 없음" 을 구분해야 하는데 client 는 그 구분을 할 수 없다.
- **주소창으로 다른 파일 열기**(`#tasty-nav:addr:`): mirror 문서에서는 **주소창을 비활성**한다.
  원격 파일시스템을 임의로 탐색하는 일은 explorer/File Picker 가 가진 별도 표면이고, 이 채널은
  "이 surface 가 지금 열고 있는 문서" 하나만 나른다.
- **원격 문서 편집·저장**: markdown surface 는 애초에 뷰어다. 스코프 밖이 아니라 기능이 없다.

### 8. plugin↔host 다리의 이름 — `markdown_mirror.*`

항목 2 가 정한 `markdown_content_request`/`markdown_content_result` 는 **client↔server** 의 wire
이벤트 이름이다. 그런데 그 원문이 실제로 그려지려면 다리가 하나 더 필요하다 — client 안에서
**markdown plugin 과 host 사이**다. plugin 은 원격 경로를 읽을 수 없으므로 host 에게 물어야 하고,
host 는 받은 원문을 plugin 에게 돌려줘야 한다. 그 다리의 이름을 여기서 정한다:

- `markdown_mirror.content_request` — plugin → host **메서드**. 이 surface 의 원문을 달라고 묻는다.
  인가는 `FsRead`(`crates/tasty-ipc/src/method_meta.rs`) — 원격 파일이라도 읽기는 읽기다.
- `markdown_mirror.content_result` — host → plugin **이벤트**. 회신 원문. host 가 `surface_id` 를
  **로컬 id 로 바꿔** 보낸다(plugin 은 원격 id 를 모른다).
- `markdown_mirror.changed` — host → plugin **이벤트**. 원문은 안 실린다. plugin 은 stale 표시만
  켜고 다시 받지 않는다 — 항목 5 가 정한 "자동 재수신 없음" 이 이 다리에서도 같다.

**wire 이름과 따로 두는 이유**: 두 다리는 끝점도 인가 축도 다르다. wire 쪽은 attach 채널의 raw JSON
`event` 태그이고 인가는 `client_holds_workspace` 다. plugin 쪽은 plugin IPC 이고 인가는 매니페스트
권한이다. 한 이름을 양쪽에 쓰면 "어느 인가가 이 호출을 봤는가" 가 이름으로 안 갈린다.

**`markdown.` 이 아니라 `markdown_mirror.` 인 이유**: `markdown.*` 는 로컬 문서를 여는 기존
네임스페이스라, 거기에 얹으면 로컬 경로로 부를 수 있는 것과 mirror 에서만 뜻이 있는 것이 한 이름
공간에 섞인다.

## Consequences

- **얻은 것**: mirror 워크스페이스의 markdown surface 가 빈칸 대신 실제 문서를 보여준다. client 가
  **자기 테마·자기 폰트·자기 recent** 로 렌더하므로 로컬 문서와 같은 화면이 된다. 페이로드는
  원문 문자열 하나뿐이라 mesh 프레임 mirror 보다 훨씬 싸고, 렌더 채널을 webview 로 옮긴
  [ADR-0065](0065-markdown-webview-render-channel.md) 를 되돌리지 않는다. 새 인가 모델·새 권한
  토큰이 없다.
- **잃은 것**: 상대경로 이미지·링크가 깨진 채로 보인다(항목 7). mirror 문서에서 주소창이 안 먹는다.
  직렬화 700 KiB 를 넘는 문서는 잘려서 보이고(이스케이프가 많으면 원문 기준으로는 그보다 훨씬
  일찍 걸린다), 코드펜스·표 중간에서 잘리면 그 아래 렌더가 무너진다.
  client 에 markdown plugin 이 없으면 지금과 똑같이 빈칸이다. 서버 plugin 이 대용량 확인 대기 중인
  파일도 예산 안에서 읽혀 나간다(항목 4).
- **핸드셰이크 디스크립터의 `file` 이 비어 도착할 수 있다.** 그 값의 출처는 plugin 이
  `surface.create` 응답으로 올리는 snapshot 인데, 그 도착은 `tab.create` IPC 의 반환보다 **뒤**다
  — surface 를 막 연 직후에 attach 하면 `role` 은 맞지만 `file` 이 빈 문자열이다. 이것은 항목 3
  의 "파일 없이 열린 surface" 와 **wire 상 구별되지 않는다**(둘 다 빈 문자열). host 가 snapshot 을
  기다리는 선택지는 없었다 — 핸드셰이크는 동기 경로이고, 기다리면 attach 전체가 plugin 응답에
  묶인다. 그래서 client 쪽 처방은 재조회다: 디스크립터의 `file` 은 표시용 힌트로만 쓰고, 원문은
  항목 2 의 채널로 가져온다(그 회신의 `file` 은 요청 시점 값이라 채워져 있다).
  `tests/attach_markdown_content_loopback.rs` 의 `attach_when_descriptor_has_file` 이 실제 client 와
  같은 방식(붙었다 떼기)으로 이 창을 넘긴다 — 테스트만의 우회가 아니라 이 성질의 관측이다.
- **운영 비용 / 유지 부담**: `markdown_content_request`/`markdown_content_result`/`markdown_changed`
  세 이벤트 이름과 role 문자열 `"markdown"` 이 서버(`src/core/attach_runtime.rs`)와
  client(`src/app/attach_client.rs`) 양쪽에 리터럴로 중복 정의된다 — git-viewer 가 이미 같은
  형태이고([ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md) Consequences), 동기화는
  커밋 시점 수동 관리다. **같은 사실이 적힌 자리는 그 둘이 끝이 아니다** — 항목 8 의 세 이름
(`markdown_mirror.content_request`/`.content_result`/`.changed`)이 host
(`src/adapters/ipc/handler/markdown_mirror.rs`)와 plugin
(`crates/tasty-plugin-markdown/src/main.rs` 의 상수 셋)에 다시 리터럴로 갈라져 있고,
`crates/tasty-ipc/src/method_meta.rs` 의 인가 표가 셋째 자리다. 하나를 고치면 셋을 함께
옮긴다. `attach_content_info()` 는 `RemoteSurface` **전체**에 붙으므로, 화이트리스트
  (항목 1)를 넓히지 않는 한 다른 webview kind 는 계속 placeholder 다 — 새 kind 를 이 채널에 태우려면
  그 화이트리스트를 명시적으로 늘려야 한다.

## Alternatives Considered

- **A. markdown 을 egui-mesh 화이트리스트로 되돌린다** — 기각. webview kind 에는 plugin 이 mesh 를
  그릴 채널 자체가 없어(`webview_chrome.rs` 는 host fallback) 되돌리려면
  [ADR-0065](0065-markdown-webview-render-channel.md) 를 통째로 뒤집어야 하는데, 그 ADR 의 재검토
  트리거(타이포그래피·mermaid·상류 라이브러리 변화) 중 어느 것도 충족되지 않았다. mesh 로 얻는
  것("서버가 그린 그대로")은 이 채널이 잃는 것도 아니다 — 문서 렌더는 결정적이라 같은 원문이면
  같은 화면이다.
- **B. 서버 plugin 이 만든 HTML 을 통째로 보내 client host 가 webview 에 그대로 싣는다** — 기각.
  client 에 markdown plugin 이 없어도 되고 테마·타이포가 서버와 100 % 일치한다는 장점이 있지만,
  (a) 페이로드가 원문보다 크다(이미지 인라인까지 들어가면 원문의 몇 배다 — ADR-0249 는 로컬
  이미지를 문서 안에 싣는다), (b) 테마가 **서버 것으로 고정**돼 client 의 라이트/다크 설정이
  무시된다, (c) client 의 recent 목록·주소창 상태가 반영되지 않는다. 즉 "client 가 그린다" 는 이
  코드베이스의 mirror 방향(File Picker·git-viewer·explorer 셋 다 데이터를 받아 client 가 그린다)과
  어긋나면서 페이로드는 더 크다.
- **C. client 가 원격 파일을 bulk 채널([ADR-0054](0054-remote-filesystem-native-over-attach-stream.md))
  로 통째 내려받아 로컬 임시파일로 연다** — 기각. 그러면 client 의 markdown plugin 이 로컬 파일을
  여는 기존 경로를 그대로 쓸 수 있어 매력적이지만, ADR-0054 자신이 임시파일 수명·크기 상한·MIME
  안전성을 별도 설계 대상으로 남겨두었고 ADR-0059 도 같은 이유로 파일 fetch 를 스코프 밖에 두었다.
  그 정책들을 여기서 함께 정하면 근거 없는 결정이 된다. 게다가 임시파일을 열면 그 문서의 `file`
  이 **client 로컬 경로**가 되어 주소창·recent·파일 감시가 원격이 아닌 그 임시파일을 가리키는,
  더 헷갈리는 상태가 만들어진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `is_egui_mesh_allowed`(`src/core/surface_registry/egui_mesh.rs`)의 화이트리스트에 markdown 이
  다시 들어온다 — 그러면 같은 surface 가 mesh 와 content 두 채널의 후보가 되어 항목 1a 의 분기
  순서가 결정을 대신하게 되고, 그 순서는 결정이 아니라 구현 디테일이다.
- 항목 1 의 content 화이트리스트에 markdown 말고 **세 번째 kind** 가 들어온다(둘째까지는 이 결정이
  다룬다 — html 을 뺀 이유가 본문에 있다) — 그때는 "그 경로의 원문" 이라는 이 채널의 의미가 kind
  마다 다른지 다시 본다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- mirror 문서의 상대경로 이미지·링크가 깨져 보이는 것이 실사용에서 문제로 보고된다 — 그때는
  이미지 참조를 서버가 함께 실어 보내는(또는 요청 시 별도로 가져오는) 두 번째 채널을 설계한다.
  재는 법: mirror 로 문서를 여는 사용자에게 "그림이 안 보이는가" 를 묻는다 — 이 결정이 그것을
  의도한 것이라 로그에는 실패로 남지 않는다(인라이너가 base_dir 없이 조용히 건너뛴다).
- 원격 문서 **편집** 요구가 나온다 — 그때는 쓰기 방향의 인가를 새로 설계해야 하고, 그것은
  ADR-0059 가 explorer 쓰기를 미룬 것과 같은 크기의 결정이다. 재는 법: 사용자 요청.
- 700 KiB 예산이 실제 문서에서 자주 걸린다 — 재는 법: `truncated: true` 로 답한 회신의 빈도.
  지금은 그 수를 세는 자리가 없으므로, 필요해지면 세는 자리를 먼저 만든다. 특히 **이스케이프가
  많은 문서에서 원문 기준으로 예상보다 일찍 걸린다**는 보고가 나오면, 그때 고칠 자리는 예산 값이
  아니라 프레임을 쪼개는 것(청크 전송)이다 — 값만 올리면 상한 초과로 되돌아간다.

## References

- [ADR-0065](0065-markdown-webview-render-channel.md) — markdown 의 렌더 채널을 webview 로 옮긴 결정.
  **이 ADR 은 그 결정을 뒤집지 않는다** — 렌더 채널은 그대로 webview 이고, 이 ADR 은 그 webview 에
  들어갈 **원문을 어디서 얻는가**만 정한다.
- [ADR-0059](0059-explorer-remote-attach-list-dir-reuse-browse-only.md) — 같은 패턴(전용 role +
  client 가 그림 + "attach 점유 = 신뢰")의 세 번째 사례이자, 파일 **내용** fetch 를 명시적으로
  스코프 밖에 둔 문서. 이 ADR 이 그 자리를 markdown 한정으로 연다.
- [ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md) — 이벤트 쌍의 형태와 payload 예산
  캡(`GIT_QUERY_BYTE_BUDGET`)의 선례, host 가 직접 조회 로직을 부르는 형태의 선례.
- [ADR-0053](0053-native-file-picker-remote-attach-channel.md) — `list_dir_request`/`list_dir_result`
  와 "attach 점유 = 신뢰" 모델의 원조.
- [ADR-0249](0249-markdown-local-images-are-inlined-by-the-renderer.md) — 이미지 인라이너와 그
  읽기 범위. 항목 7 의 `base_dir = None` 이 그 인라이너를 무력화하는 수단이다.
- [ADR-0054](0054-remote-filesystem-native-over-attach-stream.md) — 대안 C 의 bulk 전송 채널.
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — role 목록과 채널의 현재 동작.
- [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md) — mirror 되는 것과
  안 되는 것의 사용자 관점 서술.
- 코드 근거(결정 시점의 현재 위치): `src/core/attach_runtime.rs::build_workspace_tree_surfaces` ·
  `handle_list_dir_request` · `handle_git_query_request`,
  `crates/tasty-model/src/workspace.rs::classify_attach_surfaces`,
  `crates/tasty-model/src/surface_trait.rs::attach_mesh_info`,
  `src/plugin_bridge/remote_surface.rs::RemoteSurface`,
  `src/adapters/ipc/handler/webview.rs`(`webview.set_url` — 항목 5 의 신호원),
  `crates/tasty-plugin-markdown/src/render.rs::render_document`,
  `crates/tasty-plugin-sdk/src/file_watch.rs`.
- 부분 개정: [ADR-0268](0268-a-disconnected-mirror-markdown-shows-the-disconnect-and-reconnect-refetches.md) (항목 5 의 재연결 서술, 항목 6 개정)
