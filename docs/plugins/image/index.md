# Image (`com.tasty.image`)

- **Status**: Implemented (bundled plugin)
- **주체**: 로컬 사용자 (GUI surface) · AI Agent (`tasty image` CLI)
- **배포/통합**: bundled · surface_kind(egui-mesh) · 파일 핸들러 — [plugins 개념](../../concepts/plugins.md)
- **코드**: `crates/tasty-plugin-image/`(`main.rs`/`doc.rs`/`render.rs`), 등록 `src/core/surface_registry/egui_mesh.rs`(화이트리스트)
- **권한**: 매니페스트 `permissions`
- **결정**: [ADR-0028](../../adr/0028-plugin-egui-mesh-render-channel.md)(egui-mesh 채널) · [ADR-0030](../../adr/0030-image-egui-mesh-bitmap-texture.md)(image mesh-only 개정)
- **화면**: [아래 절](#화면)

> **예제로서**: egui-mesh surface 가 **비트맵 텍스처 + chrome 을 함께** 그리는 예제 — plugin 이 자기 egui `Context` 에서 tessellate 한 mesh 를 host 가 합성한다(mesh-demo 는 순수 위젯 PoC, image 는 텍스처 포함). 새 egui-mesh surface 시작점 → [plugin-development](../../dev-guide/plugin-development.md#surface-kind--rendering-3-종).

## 목적

이미지를 보고 **임시로** 그리는 **`image` surface 종류**(뷰어 + 그림판)를 제공한다. 변경을 파일로 저장하는 것은 그 위에 얹은 부가 기능이다 — 정체성이 아니다([ADR-0245](../../adr/0245-an-image-surface-edit-is-temporary-and-is-not-restored.md)). `rendering = "egui-mesh"` — plugin 이 비트맵을 자기 egui `Context` 의 텍스처로 올려(폰트 atlas 와 동일 `TexturesDelta` 채널) chrome 과 함께 mesh 로 tessellate 하고, host 가 합성한다. 별도 Canvas 레이어는 없다([ADR-0030](../../adr/0030-image-egui-mesh-bitmap-texture.md)).

## 내부 동작

- **surface_kind `image` (egui-mesh)** — plugin(`ImageDoc`)이 픽셀·편집 상태·zoom/pan 을 소유하고, 원본 이미지 + 편집 오버레이 + floating selection 을 텍스처로 올려 viewer/paint chrome(control bar·paint bar·8 handles·zoom)과 함께 그린다. host `EguiMeshSurface` stand-in 은 파일·display_name·영속화만. 파일 로드 또는 빈 캔버스(그림판 모드 진입).
- **파일 핸들러** — `detector "image"`(확장자 규칙) + `handler` `open_surface{surface_kind:"image"}`. 이미지 파일 열기 시 이 surface.
- **여는 포맷은 컴파일된 디코더에서 파생된다** — `is_image_file`(디렉토리 순회 · `image.next`/`prev` 의 좌변)이 `image` 크레이트의 `ImageFormat::reading_enabled()` 로 판정하므로, 목록을 손으로 적는 자리가 없다. 늘리려면 `crates/tasty-plugin-image/Cargo.toml` 의 `image` feature 를 고친다. SVG 는 그 크레이트가 래스터 전용이라 대상이 아니다 — 별도 렌더러가 있어야 열린다. 디코드가 실패하면 빈 캔버스가 되므로 그 자리에서 `warn` 을 남긴다.
- **undo/redo 는 paint chrome 의 버튼으로만 실행된다 — 단축키가 없다.** 호스트
  `KeybindingSettings` 에 `image_undo`/`image_redo` 필드가 있었으나 그 값을 실행에 잇는
  코드가 어디에도 없었고, SoT 에 등록돼 있다는 이유만으로 webview 키 포워딩이 `ctrl+z` 를
  host 로 가져가 **페이지 자신의 undo 까지 죽였다**. 그래서 걷어냈다. 단축키를 주려면
  다른 plugin 과 같은 자리 — 매니페스트의 `[[contributes.commands]]` — 에 선언한다
  (`crates/tasty-plugin-git-viewer/tasty-plugin.toml` 이 그 형태다). 그때 그 항목은
  Settings > 단축키 > Plugins 서브탭에서 관리된다.
- **cli / IPC** — `image.save`/`export_png`/`paste`/`next`/`prev`/`reload` 는 plugin 이 직접 처리(픽셀·편집·네비 상태 소유), `image.open`(surface 변환)·`image.list`(host surface 열거)는 host 로 trampoline.
- **idle auto-reload(입력 없이도 갱신)** — egui-mesh surface 는 입력·geom·theme·focus·invalidated 중 하나가 있어야 host 가 `set_context` 를 forward 하므로, 아무도 안 건드리는 동안은 `paint` 가 오지 않는다 — 그래서 별도 감시 스레드가 유일한 자동 갱신 경로다. 기계는 SDK 의 `file_watch` 공용 모듈이고([plugin 개발 가이드](../../dev-guide/plugin-development.md)), 변경을 감지하면 `self_invoke` 로 이 plugin 자신의 `image.reload` 를 부른다 — 실제 read 가 그 한 경로로만 수렴해 stale read 레이스가 없다.
- **판정자는 `StatGatedDigest`(2 단 게이트)** — markdown 의 `ContentDigest`(매 폴 전량 읽기)를 그대로 쓰지 않는다. 이미지는 사용자가 고르는 아무 파일이라 **읽기 비용에 상한이 없고**, 반대로 시계(mtime)만 보면 `touch`·rsync·동일 내용 재저장마다 리로드가 돈다. 이 자리에서 리로드는 읽기의 100~250 배다(디코드). 그래서 싼 `stat` 으로 먼저 거르고(정상상태 비용이 파일 크기와 무관한 O(1)), 움직였을 때만 읽어 지문을 견준다. `stat` 이 못 가르는 같은-눈금 두 번째 쓰기는 닫개 창이 잡으며, **창 안에서 도는 것은 지문이지 리로드가 아니다**(지문이 같으면 아무 일도 안 일어난다). 근거 수치와 한계는 그 타입의 소스 문서.
- **편집 중에는 미룬다** — 그리는 중에 밑그림을 갈아치우면 스트로크가 다른 그림에 얹힌다. 편집 중 도착한 변경은 표시해 두었다가 편집을 끝낼 때 반영한다(감시자는 이미 기준선을 옮겼으므로, 안 미뤄 두면 그 변경이 영구히 사라진다).

## 인터페이스

- **사용자**: 이미지 파일 열기 → image surface. 빈 캔버스로 그림판 사용.
- **AI Agent**: `tasty image …` CLI / `image.*` IPC. surface 생성은 [work-area](../../features/work-area/index.md) (`--type image`).

## 비-목표

- surface 배치/생성 도메인 — [work-area](../../features/work-area/index.md).
- 그림판 편집 도구 상세 — design-system / 구현.
- **저장하지 않은 편집의 복원** — 복원은 디스크에 저장된 것만 대상으로 한다. 미저장 편집은 복원하지 않고, 복원 시점에 알리지도 않는다 ([ADR-0245](../../adr/0245-an-image-surface-edit-is-temporary-and-is-not-restored.md)).

## Acceptance Criteria

- Given image 플러그인 활성 When 이미지 파일 열기 Then image surface 로 표시된다.
- Given `tasty image open --file <f>` Then 활성 surface 가 image kind 로 전환되어 파일을 로드한다.
- Given 빈 캔버스 Then 그림판으로 그릴 수 있다.
- Given 이미지 surface 가 열려 있고 아무 입력도 없을 때 When 그 파일이 밖에서 바뀐다 Then 1 초 안에 다시 읽어 표시한다.
- Given 이미지 surface 가 열려 있을 때 When 그 파일의 내용은 그대로인데 mtime 만 바뀐다(`touch`) Then 다시 읽지 않는다.
- Given 편집 세션이 활성일 때 When 그 파일이 밖에서 바뀐다 Then 편집 중에는 반영하지 않고, 편집을 끝낼 때 반영한다.
- Given 저장하지 않은 편집이 있는 image surface When preset·layout 으로 복원한다 Then 디스크의 원본을 다시 읽어 표시하고, 미저장 편집은 복원하지 않으며 그 사실을 알리지도 않는다.

## 화면

화면정의서 — **Image surface 화면**.

- **시각 소스**: plugin egui-mesh 자가 렌더 (비트맵=egui 텍스처) — `design-system/` 의 image surface 디자인(있으면), vendor 예정.

[작업 영역](../../features/work-area/index.md#화면) 타일 안에 열리는 이미지 뷰어 / 그림판 surface.

### 트리거

이미지 파일 열기, `image` surface 생성/전환, 또는 빈 캔버스.

### UI 요소 인벤토리

- **이미지 뷰** — 로드된 이미지 표시(맞춤/확대 등).
- **빈 캔버스** — 파일 없이 시작한 그림판.
- 탭 표시명은 파일명(빈 캔버스면 기본 "Image").

### 상태별 시각

- 로드됨 / 빈 캔버스 / 로드 실패.

### 디자인 토큰 매핑

시각 수치·토큰의 단일 출처는 `design-system/` 이다 — [시각 소스](#시각-소스).

### 갤러리 specimen

`crates/tasty-gallery/src/catalog/components/image_viewer.rs` — Layouts › `Content viewers` ›
`Image surface / canvas`. viewer(그림 fit) / no-image(fallback glyph) 두 상태를 토큰으로 전사.
3자 매핑: [design-gallery-mapping.md](../../design/systems/design-gallery-mapping.md#surface-viewers-plugins).

### 시각 소스

plugin 이 host 가 forward 한 Theme 토큰으로 자가 렌더. design-system vendor 후 링크로 교체.
