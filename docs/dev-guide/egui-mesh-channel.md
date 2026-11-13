# egui-mesh 렌더 채널

plugin 이 **자기 프로세스에서 egui 를 tessellate** 한 vector mesh 를 host 가
전용 `egui_wgpu::Renderer`로 surface 영역에 합성한다. 이 문서는 현재 형식·수명·제약을 설명한다.

## 데이터 흐름

```
[host]  surface.set_context { surface_id, width_px, height_px, ppp, raw_input, theme }
          │  (크기/ppp 변경 · 사용자 입력 · 테마 변경 · 첫 bootstrap 시에만)
          ▼
[plugin] egui::Context::run(raw_input) → tessellate(ppp) → POD 인코드
          │  SharedBuffer 에 write + commit(footer generation)
          ▼  PluginEvent::PaintFrame { surface_id, buffer_id, generation,
          │                            frame_seq, full_textures, ime_cursor }
          │  ime_cursor 는 해당 pass의 PlatformOutput::ime — host 가 OS IME 후보창 위치를
          │  정하는 데 쓰는 역방향 값(아래 "IME candidate 위치")
[host]  SharedBuffer Acquire-load → decode_paint → (ClippedPrimitive, TexturesDelta, ppp)
          → frame_seq 체인 검증(아래 "텍스처 상태 수명 + delta 체인"): delta 적용은 체인
            연속(또는 full)일 때만, mesh 채택은 참조 텍스처 상주 시 seq 불연속이어도 수행
          → 전용 egui_wgpu::Renderer 에 update_texture/update_buffers/render
          → surface 물리 rect 으로 평행이동 + scissor 합성
```

epaint 의 `serde` feature 가 꺼져 있어 paint 타입은 JSON 직렬화가 불가하다. 그래서
`Vertex`/`indices`(이미 `bytemuck::Pod`)는 바이트 직카피하고 나머지는 수동 POD 로
인코딩한다 (코덱: `mesh_wire`). `Primitive::Callback` 은 미지원(skip).

## 구성 요소 (현재 코드 맵)

| 역할 | 위치 |
|---|---|
| POD 와이어 코덱 (encode/decode_paint) | `crates/tasty-plugin-protocol/src/mesh_wire.rs` |
| 프로토콜 타입 (set_context / PaintFrame / RawInputWire) | `crates/tasty-plugin-protocol/src/protocol.rs` |
| plugin SDK 헬퍼 (run_frame / paint / encode, 폰트 atlas 소유) | `crates/tasty-plugin-sdk/src/egui_surface.rs` (`egui-mesh` feature) |
| host 합성 (decode + 전용 Renderer + scissor) | `src/gfx/gpu/egui_mesh_prepare.rs` (`gpu.rs::render` 가 호출) |
| host→plugin set_context + 입력 forward | `src/view/main/egui_mesh.rs` |
| paint_frame 수신 라우팅 / 송신 헬퍼 | `crates/tasty-host-plugin/src/manager/{pump,events,buffer}.rs` |
| 보조 핸들 채널 (shared buffer 핸들 전송) | host `crates/tasty-host-plugin/src/handle_channel.rs` · plugin `crates/tasty-plugin-sdk/src/handle_channel.rs` · 매핑 `crates/tasty-shm/` |
| host 측 surface stand-in | `src/runtime/egui_mesh_surface.rs` |
| 화이트리스트 + api_version gate + registry 등록 | `src/runtime/surface_registry/egui_mesh.rs` |
| PoC 소비자 | `crates/tasty-plugin-mesh-demo/` |

### 보조 핸들 채널 — shared buffer 를 plugin 에 넘기는 전송 (크로스플랫폼)

mesh는 프로세스 간 공유 메모리 버퍼로 전달하고 host가 GPU에서 합성한다. 그 버퍼를 만드는 건 host 지만
(`manager/buffer.rs::create_shared_buffer_for`), 매핑 핸들을 plugin 프로세스로 넘기는 건
메인 TCP 채널이 아니라 **보조 핸들 채널**이다(메인 채널은 fd/HANDLE 을 운반 못 함). OS 별
전송 수단만 다르고 상위 프로토콜(NDJSON `HandleAttach` + `Dirty`)은 동일하다:

| | Unix | Windows |
|---|---|---|
| 채널 | `AF_UNIX` socket | Named Pipe (`\\.\pipe\tasty-handle-…`, **overlapped I/O**) |
| 핸들 전달 | `sendmsg` + `SCM_RIGHTS`(ancillary fd) | `DuplicateHandle` 로 plugin 핸들 테이블에 복제 → 결과 HANDLE u64 를 `HandleAttach.handle` in-band |
| plugin 수신 | `recvmsg` fd → `tasty_shm::receive(Fd)` | 라인 파싱 HANDLE u64 → `tasty_shm::receive(Handle)` |

`HandleChannelMessage::HandleAttach` 의 `handle: Option<u64>` 는 Windows 전용이며,
`skip_serializing_if` 로 Unix wire 는 변경되지 않는다. 핸들 복제/매핑 자체는 `tasty_shm`
플랫폼 레이어가 담당한다. end-to-end 라운드트립 검증은
`crates/tasty-host-plugin/src/handle_channel/channel_tests_windows.rs`(Windows) ·
`channel_tests.rs`(Unix).

**Unix 수신측 fd 형태 검증은 OS 별로 기준이 다르다 (필수)**: `tasty_shm::receive` 는 넘겨받은
fd 를 소유권 편입 전에 `fstat` 으로 형태 검증한다. 이때 통과 기준이 Linux 와 macOS 에서
다르다 — Linux 의 memfd/shm fd 는 `S_IFMT == S_IFREG` 로 보이지만, **macOS 의 `shm_open` fd 는
파일시스템에 없는 커널 객체라 `S_IFMT == 0`** 으로 보인다(`st_size` 등 나머지 필드는 정상).
그래서 `platform/macos.rs` 는 `S_IFMT ∈ {0, S_IFREG}` 를, `platform/linux.rs` 는 `S_IFREG` 를
요구한다. 한쪽 조건을 다른 쪽에 그대로 복사하면 **정상 fd 가 전량 거부되어 mesh paint 가
100% 실패**하고, 그 결과는 에러 없는 빈 surface 로만 나타난다. 회귀는
`crates/tasty-shm/tests/round_trip.rs` 가 잡지만, 이 테스트는 **각 OS 에서 실제로 실행되어야**
의미가 있다 — 통합테스트라 `--lib --bins` 만 도는 CI 잡에서는 컴파일조차 되지 않는다.

**Windows overlapped I/O (필수)**: 이 채널은 full-duplex 다 — host 는 HandleAttach 를
write 하면서 동시에 reader 스레드가 plugin 의 Dirty 를 blocking read 한다. Windows 의
*동기* 파일 핸들은 같은 file object 의 I/O 를 직렬화하고 `DuplicateHandle`(try_clone)은
같은 file object 를 가리키므로, sync I/O 로 두면 reader 의 blocking `ReadFile` 이
`WriteFile` 을 막아 HandleAttach 전송이 데드락된다(→ plugin paint hang → 팝업 빈 화면).
그래서 파이프를 `FILE_FLAG_OVERLAPPED` 로 열고 per-op event overlapped I/O 로 read/write
를 비직렬화한다. 회귀 방지는 `channel_tests_windows.rs` 의
`windows_handle_channel_concurrent_read_write_no_deadlock`.

## set_context 송신 정책

정적 화면을 매 frame 무조건 보내지 않는다. surface 마다 마지막 (크기, ppp, theme,
focused) 를 추적해 **크기/ppp 변경 · 누적 입력 · 테마 변경 · 미paint(bootstrap) ·
포커스 변화 · plugin 의 `SurfaceInvalidated` 알림** 중 하나일 때만 보낸다. 포커스 변화를
트리거에 넣는 이유: egui 자체가 `ctx.input(|i| i.focused)` 로 커서 블링크·포커스 링 등
위젯 시각을 바꾸므로, 입력 없이 포커스만 잃는 경우(다른 surface 클릭)에도 재전송돼야
그 변화가 즉시 반영된다.
`SurfaceInvalidated`(`MeshForwardState::invalidated`, 유휴 갱신)는 plugin 이 out-of-band
로 감지한 변경(예: idle 상태에서 외부 파일 수정)을 host 에 알려 **입력 없이도** 재forward
를 트리거하는 유일한 plugin-발 경로다 — 아래 "idle invalidate" 절 참조. focused 를 안 쓰는
plugin 은 출력 바이트가 불변이라 SDK 출력 해시 dedup 이 PaintFrame 을 흡수 — 같은 출력의
불필요한 재합성을 줄임. plugin 이 paint 를 보낸 뒤(=`egui_mesh_frame` 존재)엔 보내지 않고, crash 로
frame 이 사라지면 다시 bootstrap 한다.

<a id="idle-invalidate-surfaceinvalidated-단계-06"></a>

## idle invalidate (SurfaceInvalidated)

위 5 개 host-side 트리거와 별개로, plugin 은 `HostHandle::notify(&PluginEvent::SurfaceInvalidated
{ surface_id })` 로 **입력과 무관하게** 재forward 를 요청할 수 있다. host 수신 스레드는
어떤 이벤트든 라인마다 waker 를 깨우므로(`process.rs`) idle(입력 없는) 상태에서도 이
알림이 도착하면 다음 tick 에서 즉시 처리된다:

1. `PluginManager::pump()` 가 이벤트를 `invalidated_surfaces` 에 누적하고
   `take_invalidated_surfaces()` 로 드레인된다(`crates/tasty-host-plugin/src/manager/pump.rs`).
2. `App::event_handler` 가 pump 직후 이를 소비해, surface_id 가 속한 `MainView` 를 찾아
   `MeshForwardState::set_invalidated()` + `mark_dirty()`(redraw 요청)를 건다
   (`src/app/event_handler.rs`).
3. 다음 `forward_egui_mesh_context` 게이트에서 `invalidated` 플래그가 (다른 트리거 없이도)
   빈 입력 `set_context` 를 1 회 통과시키고, 송신 시 플래그를 소거한다(`src/view/main/egui_mesh.rs`).
4. plugin 의 `paint_surface` 가 이 무입력 frame 을 받아 자기 상태를 재확인·재-read 한다.

`SurfaceInvalidated`는 현재도 사용할 수 있지만 이를 직접 사용하는 번들 plugin은 없다.
markdown 본문은 WebView로 렌더하며 `file_watch`가 변경을 감지하면 `self_invoke`로
`markdown.reload`를 호출한다. 이 경로는 `paint`·`set_context`를 받지 않는다.

### popup·banner 대응 — `PopupInvalidated` · `BannerInvalidated`

`SurfaceInvalidated` 는 surface 전용이다. egui-mesh popup(git-viewer/clipboard-viewer 등,
아래 "egui-mesh popup 채널")과 egui-mesh banner 도 동일하게 무입력 재-forward 가 필요한
경우가 있어(아래 "egui 내장 애니메이션과 이벤트 기반 게이팅의 상호작용" 참조)
`PluginEvent::PopupInvalidated { instance_id }` 와 `PluginEvent::BannerInvalidated
{ instance_id }` 를 별도로 둔다. 처리 경로는 surface 와 같고, 마지막에 다시 그리기를
표시하는 대상만 surface 대신 popup·banner 인스턴스다. 두 variant 는 서로 완전히 같은 모양이다(아래 설명은 popup 기준으로 읽고 이름만 바꾼다):

1. `PluginManager::pump()` 가 `invalidated_popups` 에 누적, `take_invalidated_popups()` 로 드레인.
2. `App::mark_invalidated_popups_dirty`(`src/app/event_handler.rs`, `about_to_wait()` 에서
   `mark_invalidated_surfaces_dirty` 직후 호출)가 이를 소비한다. popup instance 는 surface 와
   달리 특정 window 에 귀속되지 않으므로(단일 인스턴스 가드로 host 전체에 하나), 몇 번째
   window 가 그 popup 을 그리는지 알 수 없다 — 그래서 **전 main window** 를
   `main_windows_iter_mut()` 로 순회하며 broadcast 한다(`attach_client.rs` 의 기존
   `plugin_mesh_popup_pending_repaint` 예약 패턴과 동형).
3. 새 set_context 트리거가 아니라, popup 이 이미 갖고 있던
   `MainViewState::plugin_mesh_popup_pending_repaint`(docs/features/remote-attach/index.md — 비동기 host→plugin push 후 강제
   repaint 예약)에 그대로 얹는다. `popup_render.rs` 의 forward 게이트(`need_repaint`)가 다음
   프레임에 무입력 `popup.set_context` 를 1 회 통과시킨다 — surface 의 `invalidated` 플래그와
   동일 역할을 이미 있던 필드가 겸한다(별도 상태 필드 신설 불필요).
4. banner 도 같은 네 자리를 갖는다 — `pump()` 의 `invalidated_banners` 누적 /
   `take_invalidated_banners()` 드레인 / `App::mark_invalidated_banners_dirty` 의
   `MainViewState::plugin_mesh_banner_pending_repaint` 예약 / `banner_render.rs` 의 forward
   게이트가 popup 과 같은 `need_repaint` 항으로 소비. **다른 점은 한 자리뿐이다**:
   popup 의 `plugin_mesh_popup_pending_repaint` 는 `attach_client.rs` 두 곳에서도
   채워지는데(git-viewer 의 비동기 원격 조회 결과 뒤 강제 repaint), 그 경로는
   `com.tasty.git-viewer` 전용이고 그 plugin 은 banner 를 기여하지 않아 banner 쪽에는
   대응 자리가 없다.

## plugin self-repaint (out-of-band 상태 변경)

위 5 개 트리거(크기/ppp · 입력 · 테마 · bootstrap · 포커스)는 전부 **host-side** 요인이라, plugin 이
IPC 메서드나 파일 변경 등으로 **자기 상태를 out-of-band 로 바꿔도** host 는 새 set_context 를
보내지 않는다. 이 경우 plugin 은 SDK 의 `EguiMeshSurface::repaint_last`(popup/banner 동형)로
스스로 재-paint 한다:

- `EguiMeshCore` 가 마지막 set_context 의 **geom/ppp/theme/focused** 를 캐시한다(입력
  이벤트는 저장 안 함).
- `repaint_last(&host, run_ui)` 는 캐시된 geom/ppp 로 **빈 이벤트 + 직전 focused 보존**
  재-run → 출력이 바뀌면 `PaintFrame`(popup/banner 는 각자 `*PaintFrame`) 을 송신한다.
  재-forward 왕복은 필요 없다(1-hop). plugin 줄을 읽은 수신 스레드가 이벤트 루프를 깨우지만,
  그 깨움의 redraw 는 `pump` 가 프레임을 넣기 전에 끝날 수 있다. 그래서 `pump` 는 새
  `PaintFrame` 을 받은 surface 를 `take_fresh_frame_surfaces` 로 넘기고, App 이 그 surface 가
  보이는 창에 redraw 를 다시 요청한다(`mark_plugin_surfaces_dirty`). 창이 없는 headless 는 pump
  직후 기록을 비우고, surface 를 닫거나 frame 을 버리거나 plugin 이 끊기면 그 surface 의 기록도
  지운다. surface 전용이다 —
  popup/banner 의 `*PaintFrame` 은 이 표시를 쓰지 않는다.
- 첫 set_context 도착 전(캐시 없음)이면 no-op, 출력 무변화면 `last_hash` dedup 으로 생략.

**identity 불변식**: 재-paint 의 `events` 는 빈 배열 — `set_context.raw_input` 에 가짜
사용자 입력을 주입하지 않는다. `focused` 는 이벤트가 아니라 지속 상태이므로 직전
set_context 값을 그대로 재현한다(불변식 무위반) — false 로 떨어뜨리면 egui `has_focus()`
의 viewport 게이트가 꺼져 커서·드롭다운 등 포커스 의존 UI 가 재-paint 프레임에서만
퇴행한다(markdown 주소창 진동 버그의 원인이었다). 캐시된 theme 은 `last_theme()` 로
노출돼 plugin 이 draw closure 를 같은 토큰으로 재구성한다.

image는 `image.next`·`prev`·`paste`·`save`·`reload`(파일 감시) 뒤 이 경로로 다시 그린다.
git-viewer는 draw closure의 사용자 입력에서 상태가 바뀌므로 별도 self-repaint가 필요하지 않다.
markdown 본문은 WebView이며 확인 팝업만 egui-mesh를 사용한다.

## egui 내장 애니메이션과 이벤트 기반 게이팅의 상호작용

host 입력이 없을 때도 egui가 다음 pass를 요청할 수 있다.
SDK는 ROOT viewport의 repaint_delay를 읽어 SurfaceInvalidated·PopupInvalidated·BannerInvalidated로 다음 context를 요청한다.
출력 frame이 dedup으로 생략돼도 이 요청은 확인한다.

SelfRepaintTimer는 plugin 프로세스당 하나이며 첫 요청에 시작한다.
Condvar로 가장 이른 기한까지 기다린 뒤 lock 밖에서 인스턴스 arm을 풀고 알린다.
인스턴스당 대기 요청은 최대 하나다. 0 지연도 같은 큐를 사용하고 checked_add 범위를 넘는 기한은 버린다.
callback panic은 요청별로 기록하고 계속한다. 루프가 종료되면 대기 arm과 running 상태를 풀어 다음 요청이 재기동하게 한다.
타이머 thread 생성 실패에는 일회성 thread를 시도하고 실패를 로그로 남긴다.

0 지연을 inline 알림으로 바꾸면 frame 송신 전에 invalidation이 항상 나가는 식으로 관측 순서가 달라질 수 있다.
현재 blocking read_line plugin loop에는 host TimerHub의 프레임별 drain을 그대로 사용할 수 없다.

local surface의 raw_input.time은 mesh_time_now의 단조 경과시간을 사용한다.
attach mirror와 popup·banner의 time 값은 별도이며 현재 None 경로가 남아 있다.
wheel smoothing을 줄이는 입력 분할과 프로그램적 scroll animation 정책은 아래 입력 절을 따른다.

## Theme 스냅샷 (generic parity)

`set_context.theme` 는 host 가 resolve 한 현재 Theme 의 POD 스냅샷(`ThemeWire` =
색 집합 `ThemeColors` + `is_light` + UI zoom)이다. egui 의존이 없어 default 빌드에도
포함된다. plugin 은 `Theme::with_colors_and_zoom` 으로 host 와 동일한 `Theme` 인스턴스를
재구성해 디자인 토큰대로 그린다(sizing 은 zoom 으로 재도출). 모든 egui-mesh surface 가
공유하는 generic 필드다 — image/git-viewer 등이 같은 경로로 Theme parity 를 얻는다
(markdown 은 [WebView 호스트 계약](../design/systems/webview.md) 로 본문 surface 가
webview 전환돼 이 경로 밖 — 대신 `theme.query`/`theme.changed` 를 쓴다. 대용량/파일열기
확인 팝업 2 개는 여전히 이 경로를 탄다).
테마 변경은 위 송신 정책의 트리거이므로, 사용자가 테마를 바꾸면 입력이 없어도 재forward 된다.

## 콘텐츠 전달 (surface.create bootstrap)

egui-mesh surface 는 plugin 이 콘텐츠를 소유하므로(예: image 의 파일 경로), host 는
**첫 set_context bootstrap 직전에 `surface.create{params}` 를 plugin 에 1 회 보낸다**
(`MainView::forward_egui_mesh_context` → `send_egui_mesh_surface_create`). 같은 plugin
req 채널 FIFO 라 create 가 set_context 보다 먼저 도착해, plugin 이 생성 params 를 렌더 전에
받는다(set_context-before-create 레이스 제거). host 측 `EguiMeshSurface` stand-in 은
`file` 을 보관해 layout 영속화(snapshot→restore)에서 같은 params 를 재전달한다.

## TextureId 격리 / ppp 가드

egui-mesh surface 마다 **독립 `egui_wgpu::Renderer`** 를 둬, plugin 의
`TextureId::Managed(0)`(폰트 atlas)가 host 폰트와 충돌하지 않게 한다(remap 불필요).
디코드된 ppp 가 host 의 현재 ppp 와 어긋나면(리사이즈/DPI 전환 직후 stale) 그 frame
합성을 미뤄 잘못된 스케일 합성을 막는다.

## 텍스처 상태 수명 + delta 체인

wire 의 `textures_delta` 는 **증분**(full atlas 는 Context 첫 run 에만)이고 SharedBuffer 는
latest-wins 라, host 가 중간 frame 을 못 보면 그 frame 의 텍스처 delta(font atlas, image
비트맵 등)가 유실된다. 다음 규칙으로 처리한다:

1. **surface 수명 귀속** — 전용 Renderer/디코드 캐시는 "보이는 동안"이 아니라 **layout 에
   존재하는 동안**(전 workspace, 비활성 탭 포함) 유지한다
   (`MainViewState::egui_mesh_surfaces_existing`). 비가시 surface 의 도착 frame 도 매 tick
   디코드해 delta 체인을 유지한다(합성만 skip) — 탭/workspace 전환 후 복귀 시 재전송
   왕복 없이 즉시 정상 합성된다. 비가시 GPU 텍스처 상주는 의도된 비용이다.
2. **frame_seq 체인 검증 + full 재전송 (텍스처 delta 한정)** — plugin SDK 는 송신 frame
   마다 단조 시퀀스 `frame_seq`(buffer 재생성과 무관)를 `PaintFrame` 메타에 싣는다. host
   는 `frame_seq == last + 1` 이 아니면(관측 누락) 그 frame 에 실렸던 `textures_delta` 가
   유실됐다고 보고 **delta 를 적용하지 않는다**(`chain_accepts`). 대신 다음 set_context 에
   `need_full_textures` 를 실어 보낸다. SDK 는 자기가 보낸 텍스처 상태를 누적 보관
   (`EguiMeshCore::tex_state` — full 교체 / patch 합성 / free 제거)하다가, 이 요청에 dedup
   을 우회하고 **전체 텍스처 상태를 full image 로 동봉**한 frame 을 `full_textures = true`
   로 재송신한다. host 는 full frame 을 체인과 무관하게 수락하고 자기 텍스처 상태를
   리셋한다(full 미포함 텍스처는 free). Context 생성 직후 첫 frame 도 자연-full 로 마킹돼,
   bootstrap 직후 gen1 이 덮여도(생성 race) 같은 경로로 회복된다.

   **전체 텍스처 요청 재등록**: 요청한 full frame 이 다시 latest-wins 버퍼에서
   유실될 수 있으므로, host 는 수락될 때까지 **매 tick full 재전송을 재요청**한다(로그는
   최초 1 회만). frame 수락 시 해제되어 다음 단절 때 다시 요청·로그한다.
3. **mesh(기하) 채택은 delta 체인과 분리** — reflow frame 의 mesh 는 자기완결적 기하라
   중간 frame 유실(delta 손실)과 무관하다. 따라서 위 체인 가드는 **텍스처 delta 적용
   여부만** 막고, mesh 채택은 `decode_mesh_into_target` 이 디코드 후 `classify_decode` 로
   세 결과 중 하나로 판정한다. 입력 축은 셋: `chain_ok`(full 이거나 `frame_seq == last+1`),
   참조 상주(`all_textures_live` — 이 frame 의 mesh 가 참조하는 모든 `TextureId` 가 이미
   상주), delta 경계 정합(`deltas_fit_live` — patch delta 가 상주 텍스처 크기 안에 들어감,
   오버런 검사).

   | 결과 | 조건 | mesh | `textures_delta` | `last_seq` | full 재요청 |
   |---|---|---|---|---|---|
   | **Accepted** | `chain_ok` + 경계 정합 | 채택 | 적용 | 전진 | 없음 |
   | **AcceptedStale** | 체인 단절(seq 점프)이나 참조 상주 + 경계 정합 | 채택 | **미적용** | **미전진** | **매 tick** |
   | **NeedsFull** | 첫 콘텐츠 없음 · 미상주 참조 · 오버런(경계 초과) | 보류 | 미적용 | 미전진 | 매 tick |

   **불변식**: delta 를 실제 적용하지 못한(체인 단절으로 스킵한) frame 의 mesh 는 채택하되
   `last_seq` 를 전진시키지 않는다(**AcceptedStale**). 그러면 다음 tick 도 체인 단절로 남아
   full 재전송 요청을 계속 유지하고, 유실로 stale 해진 atlas 는 다음 full frame 으로 정합
   복구된다. mesh(기하)는 최신으로 갱신되므로 리사이즈/split 로 폭이 바뀌어 seq 가 튀어도
   mesh-demo 등 mesh surface 가 옛 폭에 고정(우측 잘림)되지 않고 즉시 reflow 되고, 그 사이
   글리프 uv 만 다음 full 까지 stale atlas 를 가리킨다. 참조가 하나라도 미상주(image plugin
   신규 비트맵 등)거나 delta 가 오버런이면 **NeedsFull** — mesh 도 보류하고 full 을
   재요청한다(오버런은 적용 시 egui-wgpu 가 리사이즈 없이 write 해 크래시하므로 보류가
   방어선이다). mesh-demo 처럼 텍스처가 없는 순수 위젯 surface 는 참조가 항상 폰트
   atlas(상주)라 단절 시 AcceptedStale 경로다.

요청 플래그의 흐름: 렌더 prepare 가 `NeedsFull` 또는 `AcceptedStale` 을 판정하면 `GpuState` 의
요청 대기열에 적재 → redraw 가 drain 해 세 채널 모두 `MeshForwardCommon::pending_full` 에
옮김(surface 는 `MeshForwardState` 안에, popup/banner 는 `MainViewState` 의
`plugin_mesh_{popup,banner}_forward` 맵 안에) → 다음 tick 의 forward 가
`need_full_textures` set_context 를 송신(비가시 surface 는 마지막 geom/theme 으로 송신).
plugin generation 이 정지해 새 frame 이 안 와도 이미 재전송 대기 중인 surface 는 매 tick 재요청을
유지한다(재-tessellation·업로드 없이 IPC 메시지만). popup/banner 도 같은 체인 규칙·재무장·
mesh 분리 규칙을 공유한다 — 규칙이 닮아서가 아니라 같은 MeshForwardCommon 상태 타입을 사용하기 때문이다.

## 입력 forward · 사용자 입력 경계

host 가 받은 **실제 사용자 입력**만 surface-local 좌표로 변환해 `set_context.raw_input`
으로 forward 한다. 포인터(클릭/스크롤/이동)에 더해 **포커스된 egui-mesh surface** 는
키보드도 받는다:

| 입력 | wire 이벤트 | 누적 지점 |
|---|---|---|
| 포인터 버튼/이동/스크롤 | `PointerButton`/`PointerMoved`/`Scroll` | `egui_mesh_push_pointer_*`/`egui_mesh_push_scroll` |
| 포인터가 surface 밖으로 나감 | `PointerGone` | `egui_mesh_push_pointer_gone`/`attach_mesh_push_pointer_gone` ← `mouse.rs` `update_mesh_hover` |
| 키 누름(press-only) | `Key { key: egui Key::name(), … }` | `egui_mesh_push_key` ← `keyboard.rs` `forward_key_to_egui_mesh` |
| 텍스트 입력 | `Text { text }` | `egui_mesh_push_text` (게이트 `should_forward_text`) |
| IME 조합(라이브 preedit + commit) | `Ime { event: ImeWire::… }` | `egui_mesh_push_ime` ← `ime.rs` `forward_ime_to_egui_mesh` |
| 복사 단축키(`egui_copy` capability 를 가진 kind 한정) | `Copy` | `egui_mesh_push_copy` ← `copy_paste.rs` `handle_copy_shortcut` |
| 붙여넣기 단축키·팔레트 붙여넣기(`egui_paste` capability 를 가진 kind 한정) | `Paste`(클립보드 내용 없음) | `egui_mesh_push_paste` ← `copy_paste.rs` `run_paste` |

**스크롤은 한 pass 에 전량 전달된다.** SDK 의 와이어→egui 매핑(`egui_surface.rs` 의
`push_scroll_events`)이 `Scroll` 한 건을 egui 의 "이미 부드러운 입력" 판정선(8pt) 아래
조각들로 쪼개 **같은 프레임의 이벤트 목록**에 넣는다. 쪼개지 않으면 egui 가 델타를
`unprocessed_scroll_delta` 에 적립해 여러 프레임에 걸쳐 소진하고, egui-mesh 에서는 그
프레임 하나하나가 `*Invalidated` → `set_context` → 전체 egui pass 라는 프로세스 간 왕복이
된다(이 문서). 조각 합은 원본
델타와 같아 이동량이 보존되고, 잔여 델타가 남지 않아 위 "유휴 상태 방치" 경로도 타지 않는다.
조각 수 상한(64)을 넘는 극단적 델타만 쪼개지 않고 그대로 넘긴다. 같은 이유로 모든 egui-mesh
Context 는 생성 시 프로그램적 스크롤 애니메이션(`Style::scroll_animation`)을 꺼 둔다.

**와이어 `Scroll` 은 언제나 논리 포인트다.** SDK 가 받은 값을 `MouseWheelUnit::Point` 인
`MouseWheel` 로 egui 에 넣으므로, host 의 수집 지점이 자기 입력 소스의 단위를 포인트로 맞춰
보낸다. 배율의 런타임 단일 출처는 egui `Options::line_scroll_speed` 이고
`src/plugin_bridge/wire_scroll.rs` 의 `line_scroll(ctx)` 로 읽는다 — 세 수집 지점이 모두
그것을 읽으므로 값이 갈리지 않는다. host 는 그 옵션을 사용자 설정
(`GeneralSettings::wheel_line_scroll`, 기본 50pt)으로 채운다. 같은 옵션을 host egui 의
`ScrollArea` 전반도 쓰므로 플러그인 화면과 호스트 UI가 같은 스크롤 배율을 사용한다
([터미널 (Terminal)](../surfaces/terminal/index.md)).

| 수집 지점 | 입력 소스 | 환산 |
|-----------|-----------|------|
| egui-mesh surface (`view/main/mouse.rs`) | winit `MouseScrollDelta` | `LineDelta` × 노치 거리 · `PixelDelta` ÷ ppp |
| popup (`plugin_bridge/popup_render.rs`) | host egui `Event::MouseWheel` | `wire_scroll::wheel_delta_to_points` — `Line` × 노치 거리 · `Point` 그대로 · `Page` × 화면 높이 |
| banner (`plugin_bridge/banner_render.rs`) | host egui `Event::MouseWheel` | 위와 동일 |

두 소스는 같은 물리 입력의 다른 표현이다 — egui-winit 이 winit `LineDelta` 를 `Line`(줄 수
보존), `PixelDelta` 를 `Point`(÷ pixels_per_point) 로 옮긴다. 그래서 세 경로 모두 마우스 휠
1 notch 가 설정된 노치 거리(기본 50pt)만큼, 트랙패드는 포인트 델타 그대로다. 기본값에서는
물리 휠도 판정선(8pt)을 넘으므로 위 분할이 세 경로 모두에 관여한다 — 사용자가 노치 거리를
8pt 아래로 낮추면 물리 휠은 분할 없이 1:1 로 전달된다. 설정 창의 하한은 10pt 라 UI 로는
그 아래로 못 내려가고, `config.toml` 을 직접 고친 경우에만 도달한다.

`MainView.mesh_pointer_hover`(`Option<MeshHoverTarget>`, `Local(surface_id)`/`Attach(surface_id)`)가
마지막으로 `PointerMoved` 를 받은 mesh surface 1 개를 추적한다. `handle_cursor_moved`
(egui-mesh·attach mesh mirror 판정 지점)와 `handle_cursor_left`(`WindowEvent::CursorLeft`)가
매 `CursorMoved`/`CursorLeft` 이벤트마다 `update_mesh_hover(new_target)` 를 거쳐 슬롯을
갱신하며 — `egui_consumed`/오버레이(설정창)/팝업/배너/modifier-hint hover 로 인한
early-return 경로도 포함: 이 경로에 진입한 이벤트는 mesh 판정 자체를 건너뛰고
곧장 `update_mesh_hover(None)` 을 호출한다, 즉 "이번 프레임은 mesh surface 위가
아니다"로 취급 — 대상이 바뀌면(다른 mesh surface 로 전환되거나 `None` 이 되면)
**이전** 대상에 `PointerGone` 을 1 회 forward 한다. 안 그러면 plugin 쪽 egui 가 마지막
`PointerMoved` 위치에 포인터가 계속 있다고 착각해 hover 하이라이트가 잔류할 수 있다.

키/IME 는 **포커스된 egui-mesh surface 에만** 간다(`focused_egui_mesh_surface_id`
downcast 판정). 중앙 키 디스패처(`keyboard.rs handle_keyboard_input`)가 단축키·vi·
escape 소비를 **먼저** 처리한 뒤, 소비되지 않은 키를 이 forward 로 넘긴다 — 단축키
선점 순서는 터미널 forward 와 동일하다. IME 는 조합 중 preedit 문자열까지 나르므로
(commit-only 아님) plugin 의 egui `TextEdit` 이 조합 중간 상태를 인라인 표시한다.
image/mesh_demo의 키·IME 입력은 이 경로로 전달하며, 호스트 egui에는 넣지 않는다
(`main.rs`의 host-egui 키 입력에서 제외). 따라서 호스트 egui가 먼저 처리하지 않는다.
(markdown은 본문 surface가
[WebView 호스트 계약](../design/systems/webview.md) 로 webview 전환되어 이
경로 밖 — 대용량/파일열기 확인 팝업 2 개만 여전히 이 forward 대상이다.)

키 wire 는 egui `Key::name()` 문자열을 나르고 plugin SDK(`map_event`)가
`Key::from_name` 으로 복원한다. winit→egui `Key` 변환은 논리 키 우선·물리 키 폴백
(egui-winit 미러)이라 비-라틴 레이아웃의 편집 단축키(Ctrl+A 등)도 물리 위치로 매칭된다.
Text 게이트는 command modifier·제어/사설영역 문자·IME 조합 중 non-ASCII 를 억제한다
(조합 결과는 `Ime` `Commit` 으로 도착).

set_context 송신 자체는 host 렌더 파이프라인의 일부라 사용자 상태(focus/스크롤/선택)에
부수효과가 없다. 에이전트 IPC/CLI 가 raw_input 을 합성·주입하는 진입로는 **release 에
없다** — 입력 주입은 `#[cfg(debug_assertions)]` debug 격리(`debug.inject_window_mouse`)로만
존재한다([사용자 통제와 데이터 원칙](../principles/control-and-data.md), [debug-ipc](debug-ipc.md)).

### IME candidate 위치 — plugin 이 값을 되돌려준다

OS IME candidate 창(조합 후보 목록)의 위치를 winit 에 알리는 것은 host 뿐이지만
(`set_ime_cursor_area`), **그 위치를 아는 것은 plugin 프로세스의 egui** 다 — host egui 에는
대응 위젯이 없어 `platform_output.ime` 가 늘 `None` 이라 egui-winit 의 자동 경로가 안 탄다
(`gfx/gpu.rs` 의 `ime_widget_focused`). 그래서 값이 **역방향으로** 돌아온다: plugin egui 의
`PlatformOutput::ime` 를 `PaintFrame`/`PopupPaintFrame` 알림의 `ime_cursor:
Option<ImeCursorWire>` 칸에 실어 보내고, host 가 콘텐츠 영역의 물리 origin 을 더해 창
좌표로 올린 뒤(`plugin_bridge::mesh_ime_cursor_area`) winit 에 넘긴다. 후보창은
`cursor_rect`(주 캐럿)를 따른다 — 터미널 갈래가 anchor **셀** 사각형을 넘기는 것과 같은
의미다.

`MainView::update_ime_cursor_area` 의 입력원은 셋이고 **IME 라우팅의 선점 순서와 같은
순서로** 고른다: 키 포커스를 가진 plugin popup → 포커스된 egui-mesh surface → 터미널
preedit. 조합을 받는 쪽이 후보창 위치도 정한다.

**dedup 이 IME 위치도 본다**: SDK 는 출력 바이트 해시와 IME 위치가 모두 직전에 보낸 frame 과
같을 때만 알림을 생략한다. 그림이 같아도 입력칸 포커스가 생기거나 사라져 `ime_cursor` 가
Some↔None 으로 바뀌면 frame 을 보내므로, host 의 최신 frame 캐시가 지금 입력칸 포커스를 반영한다.

**banner 는 대상이 아니다** — 키/텍스트/IME 를 forward 받지 않으므로(포커스를 주지 않는
non-modal 공지) `BannerPaintFrame` 에는 그 칸이 없다.

### 알려진 한계 (attach mesh mirror 의 candidate 위치 · Cut/Paste)

**attach mesh mirror 는 위 경로 밖이다** — 그 frame 은 고정 길이 바이너리 chunk 헤더
(`tasty-ipc` `mesh_stream`)로 나르고 거기에는 `ime_cursor` 칸이 없다. 원격 mesh surface 를
편집할 때 후보창은 여전히 마지막 위치나 창 원점 근처에 뜬다.

Copy·Paste 는 위 표대로 각각 `egui_copy`·`egui_paste` capability 를 가진 kind 한정으로 wire 에 있다.
`Paste` 는 형식 붙여넣기 신호다. 클립보드 내용을 싣지 않고 SDK 가 내용 없는 `egui::Event::Paste` 로
바꾸므로, egui `TextEdit` 에 텍스트가 들어가지 않는다 — plugin 이 이벤트를 보고 필요한 형식을 클립보드에서
직접 읽는다. 텍스트 붙여넣기는 `PasteText { text }` 가 나르고 SDK 가 `egui::Event::Paste(text)` 로 바꿔
포커스된 `TextEdit` 에 넣는다. `Cut` 은 SDK 가 `egui::Event::Cut` 으로 바꾼다. `PasteText`·`Cut` 은 wire
와 SDK 매핑만 있고 호스트는 아직 보내지 않는다. attach mesh mirror 와 popup 미러 경로에는 Copy·Paste
전달이 없다.

복사·잘라내기 결과는 프레임과 별개인 `PluginEvent::CopyText { target, text }` 로 나간다. `target` 은
`CopyTargetWire::Surface { surface_id }` 또는 `Popup { instance_id }` 중 하나다. SDK 의 surface·popup
`paint`·`repaint_last` 는 egui 패스 직후, 출력이 같아 프레임 송신을 생략하는 판정보다 먼저 이 알림을
보낸다. 선택만 복사하면 화면이 바뀌지 않아 프레임이 생략되기 때문이다. 알린 값은 소비하므로
`take_copied_text` 로 다시 나오지 않고, 한 패스에 복사가 여럿이면 마지막 값을 쓴다. banner 는 키 입력을
받지 않으므로 대상이 아니다. host 는 이 알림을 채널의 plugin id 와 함께 일회성 큐에 담는다
(`PluginManager::take_copied_texts`, 상한을 넘으면 오래된 결과부터 버린다). 이 함수는 surface·popup
결과를 함께 꺼낸다. wire 와 결과에는 창 id 가 없으므로 GUI 에서는 pump 직후 한 번 꺼내 대상을 가진
창으로 나눈다. 수신 스레드가 알림 줄마다
이벤트 루프를 깨우므로 프레임 없이 복사 결과만 온 tick 에도 pump 직후 꺼낼 수 있다. headless host 는
GUI 클립보드가 없어 큐를 버린다. GUI 에서 큐를 꺼내 OS 클립보드에 쓰는 경로는 아직 없다.

`RawInputEventWire` 는 `#[serde(other)] Unknown` fallback 을 가진다. 이 fallback 이 있는 SDK 로
빌드한 plugin 은 모르는 입력 이벤트를 `Unknown` 으로 읽고 버리며, 같은 `set_context` 의 나머지
이벤트는 그대로 처리한다. 그래서 다음 입력 변형부터는 추가가 minor 다
([api-conventions](api-conventions.md#버전과-호환성-판단)). `Paste` 는 이 fallback 보다 먼저 들어가서,
fallback 이 없는 옛 SDK 는 `Paste` 가 실린 프레임을 읽지 못한다(CHANGELOG `(BREAK)`).

## crash 격리

plugin 프로세스가 죽으면(reader 스레드 종료 → event_rx Disconnected) host 는 그
plugin 의 `egui_mesh_frames` 를 즉시 비운다. 합성기는 frame 이 없으면 skip 하므로
surface 가 곧장 blank 로 전환되어 마지막 mesh 가 stale 합성되지 않는다. host 는 죽지
않으며, 60 초 healthcheck 가 plugin 을 재시작하면 bootstrap set_context 로 재합성된다.

### 빈 화면 감지 (host 로그)

blank 전환 자체는 정상 동작이지만, **왜** 비었는지는 host 가 알 수 없다 — plugin 의 paint
실패는 plugin 자체 로그(`tasty plugin logs <id>`)에만 남고 host 에는 통지되지 않는다. host
forward 루프는 frame 이 없는 채널을 조용히 건너뛰므로, host stderr 만 보는 사람에게는
아무 징후 없이 빈 화면만 남는다.

그래서 `MeshForwardCommon` 이 bootstrap set_context 송신 시각을 기록해두고(`record_sent`
가 첫 송신 때만 무장한다 — 재forward 마다 갱신하면 유예가 계속 밀려 빈 화면을 영영 못
잡는다), `BLANK_MESH_GRACE`(3 초)가 지나도록 frame 이 하나도 오지 않으면 대상 하나당 **1회**
(surface 는 surface 당, popup·banner 는 인스턴스 당) `ERROR` 로 그 사실과 plugin 로그 확인
경로를 남긴다. frame 이 도착하면 래치가 풀려, 이후 crash 로 다시
비면 재경고한다. 원인 자체는 여전히 plugin 로그에서 확인해야 한다 — 이 로그는 "어디를
볼지"를 가리키는 신호다.

**surface·popup·banner가 모두 사용한다** — surface·popup·banner 의 forward 루프가 각자
`MeshForwardCommon::watch_blank` 를 부르고, 대상을 지목하는 구절(`surface <sid> (kind …)` ·
`popup instance <iid> …` · `banner instance <iid> …`)만 인자로 다르다. 판정도 유예값도
경고문도 그 한 곳에서 나오므로 판정이 서로 달라지지 않도록 한다.

검사는 forward 루프(= redraw) 안에서 돈다. 완전 idle 상태면 다음 redraw 까지 지연되지만,
빈 화면을 보고 조작하는 순간 발화한다.

## 개방 정책

plugin ID 와 무관하다. 번들 plugin 과 커스텀 plugin 은 같은 조건으로 egui-mesh surface kind 를
등록한다. `register_egui_mesh_kind`(`src/runtime/surface_registry/egui_mesh.rs`)의 조건은 다음과 같다.

- 매니페스트 `api_version` 이 호스트의 `HOST_API_VERSION` 과 같다. 매니페스트 검증이 이미
  같은 값을 요구하므로 설치된 plugin 은 보통 통과한다. egui-mesh popup(`[[contributes.popup]]`)도
  같은 비교만 한다.
- kind 이름이 host 내장 kind(`terminal`·`empty`·`explorer`·`dag_graph`)가 아니다.
- 다른 plugin 이 같은 kind 이름을 살아 있는 등록으로 갖고 있지 않다. 이미 있으면 **먼저 등록한
  소유자를 유지**하고 나중 선언을 warn 로그와 함께 버린다. 이 경우 `plugin.surface_kind_registered`
  이벤트도 내지 않는다. 같은 plugin 이 다시 등록하면 기존 정의를 유지하고 성공으로 본다. 앞
  소유자가 비활성화·제거되어 정의가 철회된 이름은 새 plugin 이 등록할 수 있다.
  remote·webview kind 등록은 같은 이름을 덮어쓰므로 egui-mesh 와 규칙이 다르다.

API 버전 일치가 epaint 와이어 호환을 보장하지는 않는다. 와이어 호환은 디코더
(`tasty_plugin_protocol::mesh_wire::decode_paint`)가 frame 마다 magic·`WIRE_VERSION`·`Vertex`
크기로 검사한다. surface·popup·banner 가 같은 디코더를 쓰므로 이 검사도 plugin ID 와 무관하게
같다. 크기를 유지한 레이아웃 변경까지 검출하지는 않는다(`mesh_wire.rs` 모듈 설명). 검사에
실패한 frame 은 합성 캐시에 적용하지 않고 경고를 남긴다(`egui_mesh_prepare.rs`).

## attach mesh mirror 소비 경로

위 채널은 host 가 **자기 프로세스의 plugin** 을 구동하는 경로를 전제한다. attach(원격
피점유측)에서는 이 채널의 소비자가 하나 더 있다 — mirror 를 붙인 **client** 가 원격의
plugin 이 그린 mesh 를 자기 화면에 렌더하고, 자기 입력을 원격으로 되돌려 보낸다. 프로토콜의
메시지 구성(어떤 `StreamControl` variant 로 무엇을 나르는지)은
[attach-behavior.md "mesh mirror 채널"](attach-behavior.md#mesh-mirror-채널) 에 있다.
여기서는 이 소비자가 위 채널의 각 구성 요소를 어떻게 재사용/대체하는지만 정리한다.

- **공용 구동/relay 함수**: `src/plugin_bridge/mesh_forward.rs` 가 `PluginManager` 접근권이
  필요한 두 함수를 gui/headless 양쪽 빌드 공용으로 제공한다(`plugin_bridge` 는 gui 여부와
  무관하게 컴파일되는 bin-side glue 모듈이라 아래 세 소비처 모두가 참조할 수 있다) — `mesh_mirror.rs`
  자신은 설계상 `PluginManager` 를 모른다(registry 전용).
  - `forward_mesh_frames_for_engine(engine, mgr, stream_hub)` — `MeshMirrorRegistry` 의
    dirty/누적 입력/modifiers 를 직접 읽어 `surface.set_context` 를 구동하고(bootstrap 포함),
    새 `PaintFrame` 을 relay 까지 한다. **로컬 authoritative render loop(살아있는 window)가
    없는 engine 전용** — 아래 "헤드리스"와 "GUI parked engine" 두 소비처가 그대로 재사용한다.
  - `relay_mesh_frame_if_new(engine, mgr, stream_hub, sid, client_id)` — 이미 만들어진
    `EguiMeshFrame`(있다면)을 새 `set_context` 없이 순수 byte relay 만 한다. 위 함수의 꼬리
    로직이자, "GUI 살아있는 window" 소비처가 유일하게 재사용하는 부분(아래 참조).
- **서버측(헤드리스): `PluginManager` 직접 구동 (기존 채널 그대로 재사용)** — attach 서버가
  헤드리스(`boot::run_headless`)일 때는, 위 표의 "host→plugin set_context + 입력
  forward"/"host 합성" 두 축 중 **입력 forward 축만** 대체되고 **plugin 프로세스 구동
  자체는 기존 `PluginManager`/`crates/tasty-host-plugin` 를 그대로 쓴다** — attach 전용
  plugin 매니저가 별도로 있는 게 아니다. `src/boot/headless_plugins.rs::pump_plugins` 가
  매 tick 위 `forward_mesh_frames_for_engine` 을 단일 engine 에 대해 호출한다 — 되돌아오는
  `PaintFrame` 을 그대로 받아 `StreamTag::MeshData` 로 client 에 재중계한다 — **host 합성
  (`egui_wgpu::Renderer`, TextureId 격리, frame_seq 체인 검증)은 여기서 전혀 일어나지
  않는다**(서버는 화면이 없다). 원본 바이트를 그대로 client 에 넘길 뿐이다.
- **서버측(GUI, 살아있는 window): 로컬 redraw 의 결과를 옆에서 relay** — attach 서버가
  GUI(창 보유)일 때는 그 mesh surface 의 `set_context` 를 이미 로컬 창의 매 프레임 redraw
  (`MainView::forward_egui_mesh_context`)가 권위 있게 구동 중이다. 헤드리스처럼
  `forward_mesh_frames_for_engine` 으로 `PluginManager` 를 직접 구동하면 이 로컬
  authoritative loop 와 경합하므로 — attach client 가 요청한 (로컬과 다를 수 있는)
  width_px/height_px 로 재구동해 로컬 화면이 튈 수 있다 — App의 `relay_subscribed_mesh`
  (`src/app/view_mesh.rs`)는 위 `relay_mesh_frame_if_new` 만 재사용해 **별도
  `set_context` 를 보내지 않고 이미 만들어진 `EguiMeshFrame` 바이트만 읽어 `StreamTag::MeshData`
  로 relay** 한다. 유일한 예외는 attach 구독 대상이 로컬 어디에서도 렌더되지 않는 surface(다른
  탭/워크스페이스에 있어 로컬 target 목록에 전혀 없어 plugin 이 그 surface_id 자체를 모름)인
  경우뿐 — 이때 App이 runtime surface의 메타데이터와 원 등록 binding을 읽어
  최소 `surface.create` + `set_context` bootstrap을 보낸다. attach 진입에서 종류·plugin을
  판별하는 조회는 `EngineRef::find_mesh_surface_info` (`src/runtime/pty.rs`)이며
  `src/remote/server.rs`의 `attach_surface_for_stream`이 사용한다. 이미 렌더 중인 surface 에 새 구독(또는 명시 재전송 요청)이
  들어와 전체 텍스처가 필요하면, 직접 보내지 않고 로컬 `MeshForwardState::set_pending_full`로
  위임해 다음 tick 의 authoritative loop 가 `need_full_textures` 를 실어 보내게 한다(그
  사이엔 캐시된 델타뿐일 수 있는 frame 을 새 구독자에 흘리지 않고 건너뛴다 — 텍스처 손상
  방지). attach client 의 입력을 로컬 plugin 에 되먹이는 축(`MeshMirrorRegistry::take_pending_events`)
  은 아직 이 경로에 연결되지 않았다 — gui-as-server 에서 mesh 콘텐츠는 보이지만 아직
  인터랙티브하지 않다.
- **서버측(GUI, parked engine): 헤드리스와 동일하게 직접 구동** — macOS 에서 window 를
  최소화하면 `App::handle_minimize` 의 macOS 분기가 그 window 의 `MainView` 를 파괴하고
  engine 을 `App.engines`(`src/app/engine_registry.rs`)의 parked 관계로 바꾸며 MainViewState 를 보관한다. 이 engine 은
  더 이상 `handle_redraw` 가 돌지 않으므로 "GUI 살아있는 window" 항목이 전제하는 로컬
  authoritative loop 가 없다 — **처지가 헤드리스와 같다.** `App::about_to_wait`
  (`src/app/event_handler.rs`, plugin manager `pump()` 호출 직후)가 parked engine 전부를
  순회하며 각 engine 에 `forward_mesh_frames_for_engine` 을 호출한다(구독/입력 forward/
  full-resend 요청 자체는 `apply_mesh_context_on_owning_engine` 류의 owning-engine 순회
  패턴으로 이미 parked engine 에도 정상 반영되고 있었다 — 실제 frame 구동/relay 만
  빠져 있었다). `window_lifecycle.rs` 의 창 복원은 `EngineScanMut::unpark_first` 로 **1 개씩만**
  꺼내므로, 여러 window 가 동시에 minimize 돼 있으면 나머지는 계속 이 순회의 대상으로
  남는다 — 첫 매치에서 멈추는 owning-engine 패턴과 달리, 이 순회는 매 tick parked engine
  전부를 무조건 방문한다.
- **client측: host 합성 축의 재구현** — client 가 원본 `PaintFrame` 바이트를 받아 **자기
  화면에서** 위 "host 합성" 단계(decode → 전용 `egui_wgpu::Renderer` → surface rect 합성)를
  그대로 재현한다. `decode_mesh_into_target` 은 이 재사용을 위해 SharedBuffer 파싱 부분과
  디코드 로직을 분리했다 — attach 경로는 SharedBuffer 가 없으므로(네트워크로 바이트가 옴)
  `decode_mesh_bytes_into_target`(source-neutral)을 직접 부른다. 텍스처 delta 체인 검증
  (`frame_seq`/`chain_ok`/`AcceptedStale`/`NeedsFull`, 위 "텍스처 상태 수명 + delta 체인")
  규칙은 로컬 경로와 **완전히 동일**하게 client 에서도 적용된다 — 다만 복구 요청
  (`need_full_textures`)이 로컬 IPC 대신 `StreamControl::MeshFullResendRequest` 네트워크
  프레임으로 나간다는 점만 다르다.
- **surface stand-in**: 로컬 `EguiMeshSurface`(`src/runtime/egui_mesh_surface.rs`)의
  attach 대응은 `AttachMeshSurface`(`crates/tasty-model/src/attach_mesh_surface.rs`) —
  plugin 콘텐츠(예: image 파일 경로)를 소유하지 않는 순수 표시용 stand-in이라 `file` 필드가
  없다(원격이 이미 콘텐츠를 소유·bootstrap 한 상태이므로 client 가 재전달할 게 없다).
- **입력 forward 축의 대응 모듈**: `src/view/main/egui_mesh.rs`(로컬) ↔
  `src/view/main/attach_mesh_input.rs`(attach) — 후자는 좌표/modifier 변환 헬퍼
  (`mesh_local_point`/`mesh_modifiers`/`mesh_theme_snapshot`/`map_button`/`key_wire_event`)를
  그대로 재사용하고, 목적지만 로컬 `PluginManager` 대신 `CoreState` forward 큐(네트워크)로
  바꾼다. 클립보드 미지원 등 위 "알려진 한계" 는 attach 경로에도 동일하게 적용된다(입력
  자체가 같은 wire 이벤트를 타므로). IME candidate 위치는 **attach 쪽만 한계로 남았다** —
  값을 나르는 칸이 로컬 JSON 알림에만 있고 attach 의 바이너리 chunk 헤더에는 없다(위
  "알려진 한계").
- **서버측 재검증은 등록 정보로**: 서버(`build_workspace_tree_surfaces`, `src/remote/server.rs`)가
  `Surface::attach_mesh_info()` 후보를 `is_registered_egui_mesh_kind` 로 다시 확인한 결과만
  `role: "mesh"` 로 내려보낸다. 그 kind 가 같은 plugin 소유의 egui-mesh 로 등록돼 있어야 하며
  plugin ID 목록은 보지 않는다. 철회된 정의도 등록으로 본다. 소유자가 다르거나 등록부에 없는
  후보는 `role: "placeholder"` 로 남는다.
## plugin 작성

`tasty-plugin.toml` 의 `[[surface_kinds]]` 에 `rendering = "egui-mesh"` 를 선언하고,
SDK 를 `features = ["egui-mesh"]` 로 받아 `EguiMeshSurface::paint(&ctx.host,
&ctx.params, |egui_ctx| { ... })` 를 `Plugin::paint_surface` 에서 호출하면 된다.
인코딩과 송신은 SDK가 처리한다. 최소 예시는 `crates/tasty-plugin-mesh-demo/src/main.rs`.

## plugin 콘텐츠의 clip 규약 (surface·popup·banner 공통)

**적용 범위는 plugin 이 자기 mesh 안에 그리는 콘텐츠뿐이다.** plugin 은 host 가 잡아준
영역(surface rect / popup content_rect / banner content_rect) 안에서만 그리고, 그 영역
자체를 정하는 것은 host 다. 그래서 plugin 쪽 clip 조작은 **항상 "더 좁히기" 방향**이며,
부모 clip 과 교집합하는 API 만 쓴다.

| 목적 | API | 시맨틱 |
|---|---|---|
| `Ui` 수준 clip 좁히기 | `Ui::shrink_clip_rect(rect)` | 현재 clip 과 **교집합** |
| 단발 그리기 clip | `Painter::with_clip_rect(rect)` | 현재 clip 과 **교집합** |
| plugin 콘텐츠에서 금지 | `Ui::set_clip_rect` / `Painter::set_clip_rect` | 부모 clip **덮어쓰기** |

`set_clip_rect` 계열은 부모가 걸어둔 clip 을 파기한다. `ScrollArea` 안에서 행 단위로
쓰면 특히 위험하다 — 행 rect 는 **스크롤된 가상 콘텐츠 좌표**라, 뷰포트 밖으로 밀려난
행의 clip 이 그대로 유효해져 pane 경계를 넘어 다른 pane 위에 그려진다. 가로 상한만
의도했더라도 세로까지 함께 풀린다.

교집합 API 는 "부모가 허용한 범위 안에서만 더 좁힌다" 는 의미라 이 실수가 구조적으로
불가능하다. 갤러리 specimen 은 보통 `ScrollArea` 없이 고정 목록을 그리므로 이 결함을
재현하지 못한다 — clip 회귀는 본체 popup 에서 스크롤시켜 확인해야 한다.

> **host 셸 코드에는 적용되지 않는다.** host 는 팝업 Area 처럼 *경계를 새로 세우는* 쪽이라
> `set_clip_rect(content_rect)` 로 clip 을 **확정**하는 것이 오히려 필수다 — 근거와 절차는
> [popup-implementation.md](popup-implementation.md) 의 "콘텐츠 레이어 — `egui::Area`
> 등록" 절.

## plugin 리스트의 virtualization 규약 (surface·popup·banner 공통)

**행 리스트를 `ScrollArea` 에 담을 때는 `show_rows` 로 보이는 행만 그린다.** `show()` 는 자식
클로저를 **전부** 실행한다 — 화면 밖 행을 건너뛰는 것은 호출자 책임이다. tessellator 가 clip 밖
glyph 의 vertex 생성을 건너뛰므로 mesh 바이트는 보이는 만큼이지만, 그 앞단의 galley 조회 ·
Shape 생성 · Vec 할당 · 레이아웃 계산은 전 행에서 발생한다.

plugin 콘텐츠는 **`set_context` 를 받을 때마다 egui pass 를 통째로 다시 돈다.** 스크롤 중에는 휠
이벤트마다 set_context 가 오므로, 이 비용은 "한 번 무거운" 것이 아니라 휠을 굴리는 동안 입력마다
반복된다. 목록이 길수록 스크롤이 버벅인다. (휠 델타 하나가 여러 프레임의 스무딩으로 증폭되는
경로는 위 "스크롤은 한 pass 에 전량 전달된다" 로 닫혀 있다 — 조각 수 상한을 넘긴 델타만 예외.)

```rust
pane.spacing_mut().item_spacing.y = 0.0;
egui::ScrollArea::vertical()
    .id_salt("my_list")
    .auto_shrink([false, false])
    .show_rows(&mut pane, ROW_H, items.len(), |ui, row_range| {
        for idx in row_range {
            let Some(item) = items.get(idx) else { continue };
            ui.push_id(idx, |ui| row(ui, theme, item));
        }
    });
```

지켜야 하는 것 네 가지 — 근거·대안은
이 문서.

- **행 높이가 균일해야 한다.** `show_rows` 는 `row_height × total_rows` 로 위치를 계산한다. 상수든
  theme 파생이든 **한 프레임 안에서 모든 행이 같은 값**이면 된다. 행 함수와 높이 값이 어긋나면 행이
  겹치거나 벌어지므로, 높이 식은 헬퍼 하나로 뽑아 양쪽이 같은 것을 쓰게 한다. 높이가 행마다 다르면
  `show_rows` 가 아니라 `show_viewport` 로 직접 계산해야 한다.
- **`item_spacing.y` 는 `show_rows` **호출 전에** 0 으로 맞춘다.** `show_rows` 는 **바깥** `ui` 의
  `item_spacing` 을 읽어 `row_height + spacing.y` 로 행 간격을 잡는다 — 클로저 안에서 세팅하는
  것으로는 늦다.
- **`row_range` 는 전체 목록 기준 인덱스다.** 선택 핸들러에 그대로 넘기면 된다. 다만 `enumerate()`
  로 얻던 인덱스를 `row_range` 기준으로 옮기지 않으면 **엉뚱한 항목이 선택되는** 회귀가 난다 —
  virtualization 도입에서 가장 깨지기 쉬운 지점이라 스크롤해야 보이는 아래쪽 행으로 실측한다.
- **행마다 `ui.push_id(row_index, …)` 로 id 를 고정한다.** `show_rows` 의
  `skip_ahead_auto_ids(min_row)` 는 "행 하나가 auto id 하나를 쓴다" 를 가정한다. 행이 소비하는 id
  수가 행마다 다르면(예: 개수가 변하는 pill) 스크롤 위치에 따라 같은 행의 id 가 바뀌어, egui 가
  id 로 유지하는 hover/press 상태가 엉뚱한 행에 붙는다.

**가로로도 스크롤하는 리스트(`ScrollArea::both`)는 콘텐츠 폭을 따로 확정해야 한다.** 콘텐츠 폭은
그 프레임에 실제로 할당된 폭의 최댓값이라, 보이는 행만 그리면 스크롤 위치마다 폭이 출렁이고 가로
스크롤이 최장 행 끝에 도달하지 못한다. 전 행의 최대 폭을 **한 번 재서 캐시**하고 모든 행이 그 폭을
할당한다(캐시 키에 폰트 크기를 넣고, 데이터가 바뀌는 단일 지점에서 무효화한다).

- 할당 폭과 **칠하는 폭은 분리한다.** 행 배경(선택/hover/tint 밴드)까지 최장 폭으로 칠하면 짧은
  행의 밴드가 콘텐츠 끝까지 늘어나 시각이 달라진다. 배경은 그 행 자신의 폭으로 칠한다 — 이 측정은
  보이는 행에서만 일어나므로 virtualization 이 줄인 비용을 되돌리지 않는다.

> 갤러리 specimen 은 보통 `ScrollArea` 없이 고정 목록을 그려 이 결함을 재현하지 못한다 —
> virtualization 회귀는 clip 회귀와 마찬가지로 본체 popup 에서 스크롤시켜 확인한다.

## `ScrollArea` 의 휠 판정 영역 (surface·popup·banner 공통)

**`auto_shrink` 를 끄지 않으면 목록이 전폭으로 보여도 그 위 대부분에서 휠이 먹지 않는다.**
기본값은 두 축 모두 `true` 라, egui 는 **스크롤하지 않는 축**의 크기를 콘텐츠 크기로 줄인다
(`ScrollArea::vertical()` 이면 가로). 휠 hover 판정은 그렇게 줄어든 사각형으로 하므로,
라벨 폭이 몇 십 pt에 불과하면 나머지 넓은 영역에서는 휠을 굴려도 스크롤하지 않는다.

이것이 egui-mesh 에서 특히 눈에 띄는 이유가 있다. host 는 **휠 이벤트에 좌표를 싣지
않는다**(`egui_mesh_push_scroll`) — 좌표는 커서가 움직일 때 `PointerMoved` 로만 간다.
그래서 plugin 쪽 hover 판정이 스크롤 여부를 단독으로 결정하고, 실패해도 host 로그에는
"델타를 보냈다" 만 남아 **와이어는 정상인데 화면만 안 움직이는** 모양이 된다.

```rust
egui::ScrollArea::vertical()
    .auto_shrink([false, false])   // 판정 사각형 = 보이는 영역
    .show(ui, |ui| { … });
```

가로 축 하나만 켜져 있어도(`[true, false]`) 같은 증상이 난다 — 문제를 만드는 것은
스크롤하지 **않는** 축의 크기가 줄어들기 때문이다. 회귀 검증에서는 화면 영역의
**오른쪽 끝 근처**에서도 휠을 굴려야 한다. 가운데에서만 검사하면 좁아진 판정 영역 안에
들어가 이 문제를 놓칠 수 있다
(`crates/tasty-plugin-mesh-demo/src/main.rs` 의
`the_wheel_scrolls_the_list_from_anywhere_on_the_surface`).

<a id="egui-mesh-popup-채널-a2"></a>

## egui-mesh popup 채널

surface 뿐 아니라 **plugin popup 콘텐츠도 egui-mesh 로 자가 렌더**할 수 있다. surface
채널을 재사용하되 popup 의 셸/생명주기 차이를 반영한다.

### chrome 소유 경계 — host 가 셸, plugin 은 내용만

팝업 셸(scrim / bg_panel / border / outside-click / Esc / 단일 인스턴스 가드 / 크기)은
**host 가 소유**하고, plugin 은 **콘텐츠만 mesh 로** 그린다. 근거: [포커스 정책](../design/policies/focus.md) —
팝업 open/close/focus 같은 *사용자 조작*은 host 소유다(에이전트/plugin 이 사용자 상태를
좌우하지 않는다). host 는 `draw_plugin_popups`(`src/plugin_bridge/popup_render.rs`)에서
셸을 egui 로 그리고, plugin mesh 는 셸 내부 content_rect 에 **host egui pass 후** 합성한다
(mesh 는 content_rect 로 clip 되어 border/close 를 덮지 않는다).

### 데이터 흐름 (surface 대비 차이만)

```
[host]  popup.set_context { instance_id, width_px, height_px, ppp, raw_input }
          │  surface 와 달리 surface_id 대신 host 발급 instance_id 로 키잉
          │  raw_input 은 draw_plugin_popups 가 ctx.input 의 egui 이벤트를 content-local
          │  좌표로 변환(콘텐츠 영역 안의 포인터 + 키/텍스트/IME만) — host 가 받은 실제 입력만
          ▼
[plugin] EguiMeshPopup::paint(&ctx.host, &ctx.params, |ctx| { ... })
          │  SDK 헬퍼가 surface 와 동일한 코덱/버퍼 로직 공유(EguiMeshCore)
          ▼  PluginEvent::PopupPaintFrame { instance_id, buffer_id, generation }
[host]  popup_mesh_frames[instance_id] 갱신 → render_egui_mesh_popups 가 instance_id 로
          lookup → decode → 전용 egui_wgpu::Renderer 로 content_rect 에 합성
```

### 입력 게이트 — 키·텍스트·IME 는 최상단 popup 만 받는다

`collect_mesh_popup_input`(`src/plugin_bridge/popup_render.rs`)이 host egui ctx 에서 수집하는
이벤트 중 포인터 계열은 콘텐츠 영역/가림 판정(`accepts_pointer`)을 타고, **키·텍스트·IME
세 계열은 `has_key_focus`(= 이번 프레임 최상단 popup) 게이트**를 함께 탄다 — 아래 깔린
popup 이 Esc·문자·조합 문자를 받아 자기 UI 로 처리하면 Esc 소유권 규칙
([popup.md § Host ↔ Plugin popup z-order](../design/systems/popup.md#host--plugin-popup-z-order))
이 깨진다. 같은 값을 wire 의 `focused` 로도 실어 plugin 쪽 egui 가 커서/포커스 표시를 host
판정과 맞춘다.

**IME 는 조합 세션 네 갈래를 전부 나른다**(`Enabled`/`Preedit`/`Commit`/`Disabled`).
egui 0.31 `TextEdit` 의 Commit 분기는 `Enabled`/`Preedit` 에서 세워지는
`state.ime_cursor_range` 가 커서와 일치할 때만 텍스트를 삽입하므로, Commit 만 실으면 조합
결과가 화면 파손도 에러도 없이 사라진다. 플랫폼 차이(Linux 가 winit `Ime::Enabled` 를 무시하고
`Preedit` 수신 시 `Enabled` 를 합성하는 등)는 egui-winit 이 이미 흡수해 ctx 에 넣으므로 수집기는
갈래 구분 없이 그대로 옮긴다.

winit IME 이벤트를 터미널 PTY 로 보내지 않는 차단은 `src/view/main/ime.rs` 의
`keyboard_overlay_open()` 분기가 담당한다 — 그 경로는 popup 으로 forward 하지 **않는다**.
이벤트는 이미 host egui ctx 에 들어가 있고, popup 으로 나르는 것은 위 수집기 하나뿐이다
(두 곳에서 나르면 이중 처리가 된다).

### bootstrap 은 1회만 (불필요 paint 억제)

set_context 는 **geom 변경 · 입력 · bootstrap(미paint)** 일 때만 보낸다. 특히 bootstrap 은
1 회만 — paint frame 도착 전 매 frame 스팸하면 plugin 이 불필요하게 여러 번 paint 한다.
1 회 보내고 frame 을 기다린다(래치는 surface 와 **같은 칸**이다 —
`MeshForwardCommon::bootstrap_sent`; frame 이 보이면 해제돼 crash 후 재bootstrap). 스팸으로 첫 frame(full atlas)이 덮여도 이제는 frame_seq 체인
검증이 감지해 full 재전송으로 회복되지만(위 "텍스처 상태 수명 + delta 체인" — popup 도
동일 규칙), 회복 왕복 자체가 낭비이므로 1 회 원칙은 유지한다.

### 개방 정책

surface 와 동일하게 plugin ID 와 무관하다 — 번들·커스텀 plugin 모두 egui-mesh popup(`rendering = "egui-mesh"`)은
plugin `api_version` 이 호스트와 일치할 때만 열린다(`open_popup_instance` 게이트). 단일
인스턴스 가드로 같은 `(plugin_id, popup_id)` 는 하나만 연다(중복 open 은 기존 instance_id
반환).

### 구성 요소

| 역할 | 위치 |
|---|---|
| 프로토콜 (popup.set_context / PopupPaintFrame) | `crates/tasty-plugin-protocol/src/protocol.rs` |
| manifest popup rendering 필드 | `crates/tasty-plugin-manifest/src/types.rs` (`PopupRendering`) |
| plugin SDK 헬퍼 | `crates/tasty-plugin-sdk/src/egui_surface.rs` (`EguiMeshPopup`) |
| host 라우팅 (popup_mesh_frames / set_context 송신) | `crates/tasty-host-plugin/src/manager/{pump,events,buffer,popup}.rs` |
| host 셸 + 입력 forward + 영역 수집 + IME 커서 영역 캐시 | `src/plugin_bridge/popup_render.rs` |
| host 합성 (decode + 전용 Renderer) | `src/gfx/gpu/egui_mesh_prepare.rs` (`render_egui_mesh_popups`) |
| PoC 소비자 | `crates/tasty-plugin-mesh-demo/` (`popup_id = "demo"`) |

### plugin 작성

`tasty-plugin.toml` 의 `[[contributes.popup]]` 에 `rendering = "egui-mesh"` 를 선언하고
(`permissions` 에 `ui.popup`), `Plugin::paint_popup` 에서 `EguiMeshPopup::paint(&ctx.host,
&ctx.params, |egui_ctx| { ... })` 를 호출한다. `open_popup`/`on_popup_closed` 로
인스턴스별 상태를 초기화/정리한다. 최소 예시는 `crates/tasty-plugin-mesh-demo/src/main.rs`
의 `draw_popup`.

> egui-mesh 는 plugin popup 콘텐츠의 **유일한 렌더 채널**이다 — git-viewer /
> clipboard-viewer 가 이 채널로 자가 렌더하며, 옛 UiNode popup 렌더 경로는 존재하지
> 않는다 (`PopupRendering` 은 `egui-mesh` 단일 variant).

<a id="egui-mesh-banner-채널-a3"></a>

## egui-mesh banner 채널

surface·popup 에 이어 **plugin banner 콘텐츠도 egui-mesh 로 자가 렌더**할 수 있다. popup
채널을 형(型)으로 하되 banner 의 non-modal 공지 성질과 생명주기 차이를 반영한다. banner
전반의 정체성·위치·큐·TTL 규칙은 [`docs/design/systems/banner.md`](../design/systems/banner.md).

### chrome 소유 경계 — host 가 셸·생명주기, plugin 은 내용만

banner chrome(컨테이너/border/close X/카운트다운/그림자)과 **스택(스코프당 표시 1 + 큐 5)·
위치(스코프 콘텐츠 최상단, 탭바 아래)·z-order·dismiss 타이밍**은 전부 host 소유다. plugin 은
content_rect 안 content 만 mesh 로 그린다(popup 과 동일한 소유 경계).
plugin banner 는 host `BannerManager` 의 **같은 큐/TTL/z-order 단일 지점**을 그대로 타고,
`BannerState.content` 만 `BannerContentSource::{Host, PluginMesh{..}}` 로 분기한다 — 별도
큐를 두지 않아 생명주기 정책이 이중화되지 않는다. 동적 plugin 인스턴스는 `BannerKey`
(`Host(&'static str)` / `Plugin(instance_id)`)로 키잉해 정적 host 배너와 한 큐에서 공존한다.

### popup 대비 차이

- **non-modal**: scrim 없음, 키보드 포커스 없음. `banner.set_context` 는 content 영역 위
  **포인터/스크롤만** forward 하고 `focused=false` (키/텍스트 없음).
- **dismiss**: outside-click/Esc 없음. TTL 자동 소멸 + host 셸 우상단 close X + plugin 의
  `banner.close` IPC. persistent(TTL 없음)도 close X 는 항상 노출(무한 배너 금지).
- **TTL 작성 지침**: [배너의 자동 닫힘](../principles/feedback-and-recovery.md#배너의-생명주기와-자동-닫힘)
  원칙에 따라 배너 안에 사용자가 고를 동작(버튼 등, close X 제외)이 없으면 manifest
  `ttl_seconds` 를 주고, 동작이 있으면 주지 않는다. 배너를 띄운 이유가 사라지면 plugin 이
  `banner.close` 로 닫는다. host 는 mesh 내용을 볼 수 없어 동작 유무를 판정하지 못하므로 이
  조합을 검증·거부하지 않으며, 원칙의 예외는 plugin 작성자가 정한다.
- **높이 고정**: 너비는 스코프 폭 도킹, 높이는 manifest `size_hint.height`(없으면 기본값)
  로 host 가 셸을 고정하고 그 안을 content_rect 로 예약한다.

### 데이터 흐름 (popup 대비 차이만)

```text
[host]  banner.open { banner_id, instance_id, surface_id }   (open_banner_instance)
[host]  banner.set_context { instance_id, width_px, height_px, ppp, raw_input, theme }
          │  draw_plugin_banners(src/plugin_bridge/banner_render.rs)가 BannerManager 가
          │  그린 content_rect 슬롯을 받아 forward. geom/입력/theme 변경 + bootstrap 시만.
          ▼
[plugin] EguiMeshBanner::paint(&ctx.host, &ctx.params, |ctx| { ... })
          ▼  PluginEvent::BannerPaintFrame { instance_id, buffer_id, generation, body_cut }
[host]  banner_mesh_frames[instance_id] 갱신 → render_egui_mesh_banners 가 host egui pass
        *후* content_rect 에 mesh 를 합성(셸/affordance 위에 clip).
        body_cut 이 있으면 draw_plugin_banners 가 배너 카드 아래에 전문 툴팁을 그린다.
```

### 잘린 본문의 툴팁 — 호스트가 그린다

plugin 본문 위젯 `tasty_ui_widgets::plugin_banner_body`가 말줄임한 frame에서는 그 frame의
`BannerPaintFrame.body_cut`(`BannerBodyCutWire { text, body_rect }`)에 전문과 본문 사각형을 싣는다.
`body_rect`는 콘텐츠 영역 로컬 논리 포인트다. `EguiMeshBanner`가 `run_frame`·`repaint_last` 모두
`plugin_banner_body_host_tooltip`으로 감싸므로, 그 안의 위젯은 툴팁을 mesh에 그리지 않고 잘림만
기록한다. SDK가 frame 뒤 `take_plugin_banner_body_cut`으로 꺼내 알림에 싣는다.

- host는 `EguiMeshFrame.banner_body_cut`에 최신 값을 두고, `draw_plugin_banners`가 본문 사각형을
  창 좌표로 옮긴다. 포인터가 본문 위에 `tooltip-delay`만큼 머물면 `show_plugin_banner_body_tooltip`이
  배너 카드 아래 `tooltip-offset`에 본문 왼쪽을 맞춰 툴팁을 그린다. 툴팁은 mesh 밖이라 콘텐츠 영역에
  잘리지 않고 포인터를 덮지 않는다.
- 값은 mesh와 같은 dedup을 따른다. 출력이 직전과 같아 알림을 보내지 않은 tick은 직전 값이 맞다.
  잘림은 본문 글이나 폭이 바뀌어야 달라지고, 그때는 mesh도 달라지기 때문이다.
- 칸이 없는 구버전 plugin 알림은 `None`이며 host는 툴팁을 그리지 않는다. 한 frame에 잘린 본문이 여럿이면
  마지막에 그린 하나만 실린다.
- 채널 밖(SDK를 거치지 않은 egui `Context`)에서는 위젯이 본문 아래에 직접 툴팁을 띄운다.

TTL 만료·close X 는 `BannerManager` 가 감지해 `closed_plugin_banners` 로 실어내고,
`draw_plugin_banners` 가 `state.plugin_banner_closes` 에 적재 → 메인 루프가
`close_banner_instance`(→ `banner.closed`)로 전파한다. plugin 이 죽어 mgr 에서 사라진 배너는
`draw_plugin_banners` 가 host UI 에서 제거(양방향 reconcile).

<a id="개방-정책--scope-소유권-d1"></a>

### 개방 정책 · scope 소유권

surface/popup 과 동일하게 plugin ID 와 무관한 api_version 게이트다(`open_banner_instance`). 트리거는 **IPC 전용**
(`banner.open`, 권한 `ui.banner`). scope 는 `surface` 만 — host 는 그 plugin 이 **소유한**
surface 에만 배너를 허용한다(`open_plugin_banner` 가 surface→plugin 매핑으로 검증). 단일
인스턴스 가드로 같은 `(plugin_id, banner_id)` 는 하나만 연다.

### 구성 요소

| 조각 | 위치 |
|------|------|
| 프로토콜 (banner.set_context / BannerPaintFrame·BannerBodyCutWire / open·closed) | `crates/tasty-plugin-protocol/src/protocol.rs` |
| manifest banner 기여 | `crates/tasty-plugin-manifest/src/types.rs` (`BannerContribute` / `BannerRendering`) |
| plugin SDK 헬퍼 | `crates/tasty-plugin-sdk/src/egui_surface.rs` (`EguiMeshBanner`) |
| host 라우팅 (banner_mesh_frames / set_context 송신) | `crates/tasty-host-plugin/src/manager/{pump,events,buffer,banner}.rs` |
| host 셸·큐·content 분기 | `src/adapters/ui/banner.rs` (`BannerContentSource` / `BannerKey`) |
| host 입력 forward + 영역 수집 + reconcile + 잘린 본문 툴팁 | `src/plugin_bridge/banner_render.rs` |
| host open/close 처리와 소유 검증 | `src/app/dispatch/plugin_banner.rs` |
| host 합성 (decode + 전용 Renderer) | `src/gfx/gpu/egui_mesh_prepare.rs` (`render_egui_mesh_banners`) |
| PoC 소비자 | `crates/tasty-plugin-mesh-demo/` (`banner_id = "status"`) |

### plugin 작성

`tasty-plugin.toml` 의 `[[contributes.banner]]` 에 `rendering = "egui-mesh"` + `scope =
"surface"` 를 선언하고(`permissions` 에 `ui.banner`), `Plugin::paint_banner` 에서
`EguiMeshBanner::paint(&ctx.host, &ctx.params, |egui_ctx| { ... })` 를 호출한다.
`open_banner`/`on_banner_closed` 로 인스턴스별 상태를 초기화/정리한다. plugin 은
`banner.open { banner_id, surface_id }`(자기 소유 surface) 로 띄우고 `banner.close
{ instance_id }` 로 닫는다. 최소 예시는 `crates/tasty-plugin-mesh-demo/src/main.rs` 의
`draw_banner`. debug 검증은 `debug.plugin_banner.open/close`.

배너 콘텐츠는 항상 배너 문맥 안에서 그려진다. `EguiMeshBanner::run_frame`이 프레임 전체를
`tasty_ui_widgets::banner_surface_ctx`로 감싸므로 작성자는 아무것도 감싸지 않는다. 공용
`tasty_ui_widgets::Button`의 Secondary 변형은 배너 위에서 `banner_button_bg`·`banner_button_border`
박스로 그려진다. 같은 plugin의 surface·popup에는 이 문맥이 적용되지 않는다
([배너 위 버튼](../design/systems/banner.md#배너-위-버튼)). mesh-demo의 `BUMP` 버튼이 이 경로의 예제다.

> 현 단계는 채널 **인프라 + 검증용 더미 PoC banner** 까지다. 실제 소비자 전환은 별도 작업.

## 이미지 텍스처의 전송

image는 원본·편집 레이어·floating selection을 egui texture로 올리고 툴바·핸들과 같은 mesh로 합성한다.
별도 Canvas/SharedBuffer 비트맵 레이어를 두지 않는다. TexturesDelta는 변경된 texture만 전송하므로
정적인 이미지의 pan·zoom은 캐시 texture를 참조하는 geometry를 보낸다.
최초 대형 이미지 업로드와 편집 레이어 갱신 비용은 남는다.
ImageDelta.pos는 부분 업로드를 표현할 수 있으나 image 편집의 부분 갱신 최적화를 구현된 기능으로 가정하지 않는다.
데이터·undo·저장은 [image plugin](../surfaces/image/index.md)이 소유한다.
