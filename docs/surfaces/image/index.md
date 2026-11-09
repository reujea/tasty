# Image (`com.tasty.image`)

- **Status**: Implemented (bundled plugin)
- **주체**: 로컬 사용자 (GUI surface) · AI Agent (`tasty image` CLI)
- **배포/통합**: bundled · surface_kind(egui-mesh) · 파일 핸들러 — [plugins 개념](../../concepts/plugins.md)
- **코드**: `crates/tasty-plugin-image/`(`main.rs`/`doc.rs`/`render.rs`/`picker.rs`), 등록 `src/runtime/surface_registry/egui_mesh.rs`(API 버전·이름 충돌 검사)
- **권한**: 매니페스트 `permissions`; 저장 경로 선택도 기존 `fs.read`로 호스트 목록만 읽으며 호스트에 쓰기 권한을 추가하지 않는다.
- **결정**: [egui-mesh 렌더링](../../dev-guide/egui-mesh-channel.md) — 이미지 렌더링과 공통 메시 전송 방식
- **화면**: [아래 절](#화면)

> **예제로서**: egui-mesh surface 가 **비트맵 텍스처 + chrome 을 함께** 그리는 예제 — plugin 이 자기 egui `Context` 에서 tessellate 한 mesh 를 host 가 합성한다(mesh-demo 는 순수 위젯 PoC, image 는 텍스처 포함). 새 egui-mesh surface 시작점 → [plugin-development](../../dev-guide/plugin-development.md#surface-kind--rendering-3-종).

## 목적

이미지를 보고 **임시로** 그리는 **`image` surface 종류**(뷰어 + 그림판)를 제공한다. 변경을 파일로 저장하는 것은 그 위에 얹은 부가 기능이다 — 필수 기능은 아니다(아래 기능 계약). `rendering = "egui-mesh"` — plugin 이 비트맵을 자기 egui `Context` 의 텍스처로 올려(폰트 atlas 와 동일 `TexturesDelta` 채널) chrome 과 함께 mesh 로 tessellate 하고, host 가 합성한다. 별도 Canvas 레이어는 없다([egui-mesh 렌더 채널](../../dev-guide/egui-mesh-channel.md)).

## 내부 동작

- **surface_kind `image` (egui-mesh)** — plugin(`ImageDoc`)이 픽셀·편집 상태·zoom/pan 을 소유하고, 원본 이미지 + 편집 오버레이 + floating selection 을 텍스처로 올려 viewer/paint chrome(control bar·paint bar·8 handles·zoom)과 함께 그린다. host `EguiMeshSurface` stand-in 은 파일·display_name·영속화만. 파일 로드 또는 빈 캔버스(그림판 모드 진입).
- **파일 핸들러** — `detector "image"`(확장자 규칙) + `handler` `open_surface{surface_kind:"image"}`. 이미지 파일 열기 시 이 surface.
- **큰 그림은 타일로 나눠 올린다** — egui 는 한 변이 `max_texture_side`(plugin `Context` 의 입력값, host 가 따로 주지 않아 egui 기본 2048)를 넘는 텍스처를 받으면 debug 빌드에서 패닉한다(`debug_assert`). 원본·그리기 층·floating selection 은 `tiled::TiledTexture` 가 한 변이 그 값 이하인 타일로 나눠 올리고, 각 타일을 그림 사각형의 해당 부분에 그린다. 확대가 핵심이라 줄여 올리지 않는다 — 원본 픽셀을 그대로 쓴다. 이웃 타일과 맞닿는 변에는 1px 테두리를 더 담고 uv 에서 뺀다. 그래서 선형 보간이 경계에서도 실제 이웃 픽셀을 섞어 확대해도 이음매가 생기지 않는다. 그리기 층은 획마다 전체 타일을 다시 올린다(그림 크기만큼의 업로드는 타일 전과 같다).
- **여는 포맷은 컴파일된 디코더에서 파생된다** — `is_image_file`(디렉토리 순회 · `image.next`/`prev`의 대상 판정)이 `image` 크레이트의 `ImageFormat::reading_enabled()` 로 판정하므로, 목록을 손으로 적는 자리가 없다. 늘리려면 `crates/tasty-plugin-image/Cargo.toml` 의 `image` feature 를 고친다. SVG 는 그 크레이트가 래스터 전용이라 대상이 아니다 — 별도 렌더러가 있어야 열린다. 디코드가 실패하면 빈 캔버스가 되므로 그 자리에서 `warn` 을 남긴다.
- **undo/redo**는 그림판의 버튼으로만 실행하며 단축키는 없다. 단축키를 추가한다면
  매니페스트 `[[contributes.commands]]`로 선언하고 Settings › Keybindings › Plugins에서
  관리한다. 사용하지 않는 호스트 바인딩을 두면 webview가 처리해야 할 Ctrl+Z까지 가로챌 수 있다.

- **cli / IPC** — `image.save`/`export_png`/`paste`/`next`/`prev`/`reload` 는 plugin 이 직접 처리(픽셀·편집·네비 상태 소유), `image.list`(host surface 열거)는 plugin 이 받아 host 로 trampoline 한다. `image.open`(surface 변환)은 GUI host 가 plugin 에 넘기지 않고 직접 변환한다. 변환은 옛 surface 의 회수 확인을 이 plugin 의 처리 스레드에서 받아야 하므로, 처리 스레드가 `host.call` 로 되돌려 보내며 기다리면 서로 기다리다 호출 시한(60초)까지 멈춘다. 헤드리스 host 에는 변환 처리가 없어 plugin 을 거쳐 `-32017` 로 거절된다.
- **idle auto-reload(입력 없이도 갱신)** — egui-mesh surface 는 입력·geom·theme·focus·invalidated 중 하나가 있어야 host 가 `set_context` 를 forward 하므로, 아무도 안 건드리는 동안은 `paint` 가 오지 않는다 — 그래서 별도 감시 스레드가 유일한 자동 갱신 경로다. 감시 구현은 SDK의 `file_watch` 공용 모듈이며([plugin 개발 가이드](../../dev-guide/plugin-development.md)), 변경을 감지하면 `self_invoke` 로 이 plugin 자신의 `image.reload` 를 부른다 — 실제 read 가 그 한 경로로만 수렴해 stale read 레이스가 없다. reload 는 `repaint_last` 로 새 frame 을 보내고, host 는 새 frame 을 받은 surface 의 창을 다시 그린다([egui-mesh 채널](../../dev-guide/egui-mesh-channel.md)).
- **변경 확인은 `StatGatedDigest`를 사용한다.** 이미지는 파일이 클수록 읽기 비용이 커지므로,
  markdown의 `ContentDigest`처럼 매번 파일 전체를 읽지 않는다. 평소에는 `stat`으로
  파일 크기와 수정 시각을 확인하고, 값이 바뀌었을 때 파일을 읽어 내용 해시를 비교한다.
  내용이 같으면 `touch`나 동일 내용 재저장만으로 다시 디코딩하지 않는다.

  같은 수정 시각 안에 두 번 저장될 수 있어 쓰기 직후에는 추가 확인 기간을 둔다.
  이 기간에는 `stat` 값이 그대로여도 내용 해시를 다시 비교한다. 해시가 같으면
  재로드하지 않는다. 확인 기간과 읽기·디코딩 비용의 근거는 SDK `file_watch`의
  `StatGatedDigest` 소스 문서를 따른다.
- **편집 중에는 미룬다** — 그리는 중에 밑그림을 갈아치우면 스트로크가 다른 그림에 얹힌다. 편집 중 도착한 변경은 표시해 두었다가 편집을 끝낼 때 반영한다(감시자는 이미 기준선을 옮겼으므로, 안 미뤄 두면 그 변경이 영구히 사라진다).

## 인터페이스

- **사용자**: 이미지 파일 열기 → image surface. 빈 캔버스로 그림판 사용.
- **AI Agent**: `tasty image …` CLI / `image.*` IPC. surface 생성은 [work-area](../../features/work-area/index.md) (`--type image`).

## 비-목표

- surface 배치/생성 도메인 — [work-area](../../features/work-area/index.md).
- 그림판 편집 도구 상세 — design-system / 구현.
- **저장하지 않은 편집의 복원** — 복원은 디스크에 저장된 것만 대상으로 한다. 미저장 편집은 복원하지 않고, 복원 시점에 알리지도 않는다 (아래 기능 계약).

## Acceptance Criteria

- Given image 플러그인 활성 When 이미지 파일 열기 Then image surface 로 표시된다.
- Given `tasty image open --file <f>` Then 활성 surface 가 image kind 로 전환되어 파일을 로드한다.
- Given 빈 캔버스 Then 그림판으로 그릴 수 있다.
- Given 한 변이 `max_texture_side` 를 넘는 그림(4000×3000, 9000×16 같은 극단 비율 포함) When 연다 Then plugin 이 패닉하지 않고 원본 해상도 그대로 표시·확대된다.
- Given 이미지 surface 가 열려 있고 아무 입력도 없을 때 When 그 파일이 밖에서 바뀐다 Then 1 초 안에 다시 읽어 표시한다.
- Given 이미지 surface 가 열려 있을 때 When 그 파일의 내용은 그대로인데 mtime 만 바뀐다(`touch`) Then 내용 해시를 확인하되 다시 디코딩하거나 재로드하지 않는다.
- Given 편집 세션이 활성일 때 When 그 파일이 밖에서 바뀐다 Then 편집 중에는 반영하지 않고, 편집을 끝낼 때 반영한다.
- Given 저장하지 않은 편집이 있는 image surface When preset·layout 으로 복원한다 Then 디스크의 원본을 다시 읽어 표시하고, 미저장 편집은 복원하지 않으며 그 사실을 알리지도 않는다.

### 다른 이름으로 저장의 찾아보기

폴더 버튼은 OS 대화상자 대신 Tasty 파일 선택 창의 저장 모드를 연다. PNG 필터와 현재 입력의 파일명
(없으면 `image.png`)을 사용하고, 입력 경로·원본 이미지의 절대 부모 폴더 순서로 출발한다. 둘 다 절대 부모가
없으면 플러그인 작업 폴더를 사용한다. 확정은 입력칸만 바꾸며 실제 쓰기는 카드의 [저장]이 한다.
기존 `.png` 보충과 명시 경로 덮어쓰기 규칙은 유지한다. 피커에서 기존 파일을 고르면 덮어쓰기 경고를
보지만, 이후 카드 입력을 다른 기존 파일로 바꾸는 동작에 별도 경고를 추가하지는 않는다.

요청은 즉시 접수되고 결과는 이벤트로 오므로 사용자 응답 동안 렌더·입력 스레드를 붙잡지 않는다.
취소는 입력을 보존하고 닫힌 카드·surface, 이전 요청이나 중복 결과는 적용하지 않는다. 이미 피커가
열렸으면 경고 로그를 남기고 버퍼를 유지한다. host popup의 자식이 아닌 창 범위 피커이며 자세한 계약은
[파일 선택 창](../../features/native-file-picker/index.md#플러그인의-피커-요청--즉시-접수하고-결과는-이벤트로-전달)에 있다.

## 화면

화면정의서 — **Image surface 화면**.

- **시각 소스**: plugin egui-mesh 자가 렌더 (비트맵=egui 텍스처) — 이미지 뷰어의 전용 vendor 시안은 현재 없다.

[작업 영역](../../features/work-area/index.md#화면) 타일 안에 열리는 이미지 뷰어 / 그림판 surface.

### 트리거

이미지 파일 열기, `image` surface 생성/전환, 또는 빈 캔버스.

### UI 요소 인벤토리

- **이미지 뷰** — 로드된 이미지 표시(맞춤/확대 등).
- **빈 캔버스** — 파일 없이 시작한 그림판. 파일을 연 서피스에서 새 이미지를 만들어도 원래 파일과 별개 문서다. 열려 있던 경로와 폴더 목록을 버리므로 Save는 다른 이름으로 저장 팝업을 열고, 경로 없는 `image.save`는 거절된다.
- **새 경로로 저장한 뒤** — 다른 이름으로 저장 팝업이나 새 캔버스에 경로를 준 `image.save`로 저장하면 플러그인이 별도 스레드에서 호스트 `image.open`(같은 surface, 새 경로)을 호출한다. 호스트가 탭 제목·복원 경로·`image.list` 경로를 새 파일로 바꾸고 surface를 다시 만들어, 새 문서가 그 파일을 읽고 감시한다. 확대·이동 상태는 처음으로 돌아간다. 파일을 연 문서를 다른 경로로 내보내는 `image.save`는 문서 경로를 바꾸지 않으므로 호스트에 알리지 않는다.
- **붙여넣기** — 이미지 surface 에 포커스가 있을 때 붙여넣기 단축키(`KeybindingSettings` 의 paste)나 팔레트 붙여넣기는 호스트가 `Paste` 이벤트로 보내고(`egui_paste`), 플러그인이 클립보드 이미지를 직접 읽어 `image.paste` 와 같이 떠 있는 선택으로 붙인다. 보기 모드에서도 편집 모드로 들어간다. 클립보드에 이미지가 없으면(텍스트 등) 아무것도 바꾸지 않는다.
- **저장 대상** — 경로 없는 저장(도구 모음 Save, `path` 없는 `image.save`)은 항상 PNG 로 쓴다. 확장자가 대소문자와 관계없이 `png` 인 문서(`IMG.PNG` 포함)는 자기 파일에 쓴다. JPG 등 비-PNG 문서는 같은 폴더의 같은 이름 `.png`(소문자)에 새 파일로만 쓰고(`create_new` — 판단과 쓰기 사이에 생긴 파일도 덮어쓰지 않는다) 문서를 그 파일로 옮긴다(아래 "새 경로로 저장한 뒤"와 같이 호스트에 알린다). 원본 파일은 바꾸지 않는다. 같은 이름 `.png` 가 이미 있으면 덮어쓰지 않는다 — 도구 모음 Save 는 다른 이름으로 저장 팝업을 열고 편집을 유지하며(입력칸 위에 caption accent-warning `image.save_as.exists`, 입력칸에는 같은 폴더의 다음 빈 이름 `<stem>-<n>.png`(n 은 1 부터)을 넣고 파일 이름의 stem 을 선택한다), `image.save` 는 `-32602`(`Save target already exists: <path>. Provide 'path' to save elsewhere`)로 거절한다. 판단은 `ImageDoc::save_target` 하나가 맡는다.
- **이전·다음 이미지로 넘어간 뒤** — 도구 모음 버튼과 `image.next`·`image.prev`도 같은 방식으로 호스트에 새 경로를 알린다. 탭 제목·복원 경로·`image.list` 경로가 넘어간 파일을 따른다. 편집 중에는 이동하지 않는다. 도구 모음 버튼은 눌러도 반응하지 않고, `image.next`·`image.prev`는 `-32602`(`Image is being edited: save or cancel the edit before moving to another image`)로 거절하며 문서 경로·폴더 안 위치·감시 대상을 바꾸지 않는다.
- 탭 표시명은 파일명(빈 캔버스면 기본 "Image").

### 상태별 시각

- 로드됨 / 빈 캔버스 / 이미지 없음 / 로드 실패.
- 이미지가 없으면 캔버스(bg-sidebar) 자리에 공용 상태 화면(`tasty_ui_widgets::state_screen`, 탐색기 상태 화면과 같은 배치)을 그린다. 캔버스 높이가 `explorer_state_compact_below()`(120) 미만이면 compact 한 줄이다. 글리프는 빌드 때 구운 폴리라인이다.

  | 상태 | 글리프 · 색 | 제목 · 보조 줄 | 이유 줄(mono) | 버튼 |
  |---|---|---|---|---|
  | 이미지 없음(원인 없음) | image · text-muted | `image_viewer.no_image` (text-secondary) | 없음 | 없음 |
  | 파일 없음(`NotFound`) | alertTriangle · image-error-fg | `image.state.missing` · `missing_sub` | 문서 경로 | Retry |
  | 권한 없음(`PermissionDenied`) | lock · accent-warning | `image.state.permission` · `permission_sub` | 없음 | Retry |
  | 디코드 실패(그 밖의 오류) | alertTriangle · image-error-fg | `image.state.decode` · `decode_sub` | 디코더 문구(번역 안 함) | Retry |
  | 너무 큼(`ImageError::Limits`) | image · image-too-large-fg(→ accent-warning, 상한으로 거절했을 뿐 파일은 정상이라 디코드 실패 톤과 다르다), 제목 text-primary | `image.state.too_large` · `too_large_sub`(상한 두 값, MiB) | `too_large_size` 헤더에서 읽은 실제 크기 "W × H px"(헤더를 못 읽으면 없음) | 없음 — 같은 파일은 다시 읽어도 결과가 같다. 파일이 바뀌면 감시가 다시 읽는다 |

- 상태 화면의 제목과 보조 줄은 공용 상태 화면의 균형 줄바꿈을 따른다(줄 길이를 고르게 해 한두 글자 꼬리를 남기지 않는다). 너무 큼 문구는 모든 내장 언어에서 숫자와 단위 사이를 NBSP(U+00A0)로 붙인다 — `too_large_sub` 의 "{}\u00A0px", 메모리 인자 "512\u00A0MiB", `too_large_size` 의 "{}\u00A0px". egui 는 NBSP 에서 줄을 바꾸지 않으므로 숫자와 단위가 다른 줄로 갈라지지 않는다.
- 디코드 상한: 뷰어는 디코드 전에 `image::Limits` 를 건다. 한 변 `MAX_IMAGE_SIDE` 16384px, 디코더 메모리 `MAX_DECODE_ALLOC` 512 MiB(image 크레이트 기본값을 명시)다. 넘으면 픽셀을 펼치기 전에 "너무 큼" 상태로 거절한다. 한 변 상한은 Explorer 미리보기와 같다. 메모리 상한은 미리보기(256 MiB)보다 크다 — 미리보기는 패널 폭으로 줄여 보내지만 뷰어는 원본을 그대로 타일로 올리기 때문이다. 상한 안의 그림도 디코드 버퍼 뒤에 RGBA 사본·`ColorImage`·타일 업로드가 더해지므로 메모리 최고점은 디코드 메모리의 몇 배다.

- Retry(secondary sm)는 도구 모음 새로고침과 같이 파일을 다시 읽는다. 원인은 `ImageDoc::load_failure` 에 남고 읽기에 성공하면 지운다. 파일 감시의 자동 재읽기와 이전·다음 이동은 그대로 동작한다.

### 디자인 토큰 매핑

현재 구현은 호스트가 전달하는 Theme 토큰을 사용한다. [시각 소스](#시각-소스) 참고.

### 갤러리 specimen

`crates/tasty-gallery/src/catalog/components/image_viewer.rs` — Layouts › `Content viewers` ›
`Image surface / canvas`. viewer(그림 fit) / no-image 두 상태와, `image-states` spec 의 빈 캔버스 · 원인별 로드 실패 네 칸(없음 · 권한 · 디코드 · 너무 큼) · compact 줄 · Save As 이름 충돌 카드를 전사한다. 상태 칸은 본체와 같은 공용 상태 화면이 그리며, 시안대로 3열 격자(두 줄)에 놓는다.
3자 매핑: [design-gallery-mapping.md](../../design/systems/design-gallery-mapping.md#surface-viewers-plugins).

### 시각 소스

plugin 이 host 가 forward 한 Theme 토큰으로 자가 렌더. 전용 vendor 시안은 현재 없다.
