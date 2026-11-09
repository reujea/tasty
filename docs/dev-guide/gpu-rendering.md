# GPU 렌더링 구조

터미널은 wgpu 커스텀 셰이더 파이프라인으로 그린다 — egui UI 와 별도로, 각 셀의 배경 + 글리프를 GPU 인스턴스 렌더링한다. 핵심 렌더러는 `src/gfx/renderer.rs` 의 `CellRenderer`.

## 터미널 내용과 사용자 viewport

`TerminalViewport`는 표시면 소비자가 소유하는 읽기 위치다. `Terminal`과
`TerminalState`는 사용자 offset을 저장하지 않는다. 터미널은 history의 content epoch,
보존된 첫 행의 좌표와 내용 revision을 제공한다. 출력과 앞쪽 trim이 일어나도 살아 있는
행의 좌표는 유지되고, ED3·새 terminal·1049 alternate clear는 이전 좌표를 무효화한다.

`Terminal::with_view(&viewport, |view| ...)`는 parser가 사용하는 기존 mutex를 한 번
잠그고 앵커·치수·행·모드를 같은 내용 cut으로 제공한다. 좌표를 먼저 얻고 별도 terminal
호출로 행을 읽으면 이 보장이 없으므로, 렌더·hit-test·추출은 `TerminalReadView`를 사용한다.
`with_content`는 사용자 선택 없는 live 읽기다. PTY 크기와 VT 응답 경로는 이 viewport와
독립적이다. alternate screen은 live를 표시하며 primary 앵커는 복귀할 때까지 보존한다.
대체 화면의 행 좌표는 기본 화면 history 길이와 무관한 0-origin이다. UI 검색도 대체
grid만 대상으로 하고 공개 content 검색과 IPC whole-buffer 읽기는 별도 계약으로 유지한다.
화면에 필요한 history 행만 읽으며 memory 행은 borrow하고 disk 행만 개별 로드한다.
IME 조합 보정은 dirty View 프레임에서 후보창 위치와 GPU 제출 전에 수행한다. 특정
PTY wake 종류에 의존하지 않아 global wake·직접 parser 주입·attach 출력에도 적용된다.

## 분할선 미리보기와 확정 구조

드래그 중 비율은 `MainViewState.layout_previews`에 있고 Core의 pane/surface tree를 바꾸지 않는다. renderer·divider hit-test·선택 위치·WebView bounds·이동 대상 표시와 PTY 크기 계산은 같은 View geometry를 읽는다. geometry 계산은 기존 leaf 객체를 빌리며 kind·Terminal·Pty를 복제하지 않는다. 기존 grid resize→tap→OS throttle 순서는 명시 surface 크기를 받은 실행 경계에서 유지한다.

로컬 드래그는 시작 시 live projection의 revision과 workspace/tab ID·split path·leaf ID 목록을 고정한다. 마지막 비율을 확정 명령으로 제출한 뒤에도 미리보기를 유지하며, 거절되거나 대상이 사라지면 해당 미리보기를 해제한다. 같은 leaf 배치로 split이 다시 만들어져도 이전 revision의 명령은 적용하지 않는다. 크롬 위 release와 무대 진입은 마지막 유효 비율로 제스처를 끝낸다.

mirror 비율은 서버 구조를 바꾸지 않는 로컬 표시 override다. 완료된 비율은 다음 delta·재연결로 mirror projection token이 바뀔 때까지 유지한다. 새 드래그가 취소되어도 앞서 완료한 override는 남는다. 로컬 구조 journal과 원격 전송 connection epoch의 원본이 아니다.

## 프레임 흐름

1. **Clear pass** — 배경색으로 클리어
2. **Terminal pass** — 아래 4단계 accumulator 로 모든 터미널을 한 번에
3. **egui-mesh pass** — plugin 이 자기 프로세스에서 tessellate 한 mesh 를 surface 영역에 합성 (host chrome 아래 layer). 채널 상세는 [egui-mesh-channel](egui-mesh-channel.md)
4. **egui pass** — 사이드바·탭바·팝업 등 UI 오버레이
5. **WebView 오버레이 pass** — 그림 위치가 오버레이 창이고 그 창이 있을 때만 돈다(아래 절). 그림 위치가 주 표면이면 이 pass 없이 주 표면 present 뒤에 준비 기록을 남긴다
6. **텍스처 해제** — 이번 프레임에 풀 egui 텍스처는 4·5가 모두 끝난 뒤 해제한다(`free_egui_textures`)

## WebView 오버레이 pass

native WebView 위에 egui 대상 팝업을 합성할 때의 두 번째 pass다. 모드·대상·backend 계약은 [WebView §egui 오버레이와 native WebView](../design/systems/webview.md#egui-오버레이와-native-webview)에 있다.

- **복제 시점**: 직전 동기화의 모드가 숨김이 아니고 그림 위치가 오버레이 창이며 그 창이 있을 때, `run_egui_frame`의 closure 끝(`end_pass` 전)에 대상 레이어의 shape를 복제해 그 모드·그림 위치·페이지(`planned_pages`)의 paint job을 만든다(`state::webview_overlay::prepare_paint_job`, `gpu/overlay_paint.rs`). 그림 위치가 주 표면이면 복제 없이 shape 없는 job만 만든다. `end_pass`가 레이어를 비우기 때문이다. 주 표면에는 원래대로 모두 그리므로 `ui.screenshot` 결과는 바뀌지 않는다.
- **순서**: egui 레이어 순서(뒤→앞)를 따르되 host·plugin 팝업은 그룹 단위로 `host_popup_should_render_on_top`에 맞춘다. 드롭다운은 맨 위다. plugin mesh 팝업은 `render_egui_pass_and_mesh_popups`를 오버레이 창 view로 다시 불러 같은 그룹 규칙으로 합성한다.
- **출력 영역**: shape마다 clip을 paint job의 페이지 사각형과 교차한다. plugin mesh 팝업도 `render_egui_mesh_popups`에 같은 페이지를 넘겨 primitive마다 clip을 교차한다(`clip_primitives_to_pages`). 좌표와 주 pass용 mesh 캐시는 바꾸지 않는다. 페이지 밖의 scrim·그림자·mesh 콘텐츠는 주 표면에만 그려져 같은 픽셀이 두 번 합성되지 않는다.
- **좌표·크기**: 오버레이 surface는 주 창 클라이언트 영역과 원점·물리 크기가 같아야 한다. egui pass와 mesh pass 모두 오버레이 surface 크기로 그린다. 크기가 주 surface와 다르면 그 프레임을 그리지 않고 오버레이 실패로 기록한다.
- **준비 기록**: shape가 없어도 투명으로 지운 프레임을 올린다. 보이는 오버레이 창 present가 끝나면 그 모드·그림 위치·페이지·표면 크기를 `OverlayFrame::painted`에 남긴다(`record_present`). 주 표면 job은 렌더가 오버레이 pass 전에 꺼내 주 표면 present 뒤에 같은 방식으로 남긴다. job을 만든 뒤 표면 크기가 바뀌었으면 남기지 않는다. 같은 프레임의 WebView 동기화는 이 기록이 현재 모드·그림 위치·페이지·표면 크기와 같을 때만 대상 팝업 아래 페이지를 드러낸다.
- **텍스처**: 오버레이 pass는 텍스처를 갱신하지 않고 주 pass가 올린 것을 쓴다. 해제는 두 pass 뒤다.
- **실패**: 오버레이 surface를 잃으면 한 번 다시 설정한다. 그래도 얻지 못하면 surface와 host를 함께 버리고 오버레이 실패로 기록한다. 같은 프레임의 WebView 동기화가 그 기록을 읽어 숨김으로 돌아간다.
- **조건**: 오버레이 창(`GpuState::overlay_surface`, `gpu/overlay_surface.rs`)은 WebView 동기화 뒤 창 단위 host 훅이 붙인다([WebView §창 단위 오버레이 host](../design/systems/webview.md#창-단위-오버레이-host)). Linux 투명 모드는 이 host와 오버레이 pass를 사용한다. macOS는 debug 강제 투명 모드에서만 host를 쓰며, 기본 불투명 모드와 Windows 불투명 모드는 주 표면에 그린다. host를 숨겨 두었으면 그리지 않고 기록도 남기지 않는다. 숨긴 채 준비 중인 host에 그린 그림은 페이지를 드러내는 기록(`painted`)이 아니라 host를 보일 기록(`prepared`)으로 남긴다. 창의 텍스처 형식은 주 surface와 같아야 한다. 같은 egui renderer로 그리기 때문이다.
- **크기**: `GpuState::resize`가 주 surface와 같은 크기로 오버레이 surface를 다시 설정한다. 배율 변경도 resize를 거친다.

## 렌더는 이벤트 루프 스레드에서 돈다 (제약)

winit `ApplicationHandler` 콜백은 전부 **하나의 스레드**에서 동기 실행되고, GPU 렌더는 그중 `WindowEvent::RedrawRequested` 처리 안에서 돈다. IPC 도 같은 스레드다 — IPC 서버 스레드는 요청을 메인 스레드로 넘기고 응답을 기다린다(`TcpIpcServer::dispatch_and_await` → `AppEvent::IpcReady` → `process_ipc`).

따라서 **GPU 호출 하나가 반환하지 않으면 키·마우스·IPC 가 전부 함께 멎는다.** 이건 렌더만 느려지는 게 아니라 창이 통째로 무응답이 되는 것이고, panic 이 아니므로 crash report 도 남지 않는다.

wgpu 24 기준으로 호출별 상한은 다음과 같다:

| 호출 | 상한 |
|------|------|
| `surface.get_current_texture()` | **있음** — wgpu-core 가 1 초(`FRAME_TIMEOUT_MS`)를 걸고 초과 시 `SurfaceError::Timeout` 반환 |
| `surface.configure(...)` | **없음** — Vulkan의 이전 swapchain 정리에서 device idle을 기다릴 수 있음 |
| `queue.submit(...)` | **없음** |
| `SurfaceTexture::present()` | **없음** |

상한이 없는 호출에는 wgpu 가 취소·타임아웃 API 를 제공하지 않는다. 그래서 tasty 는 이 상황을 *막지* 못하고 **관측만** 한다 — 아래 "진단" 의 stall 워치독. 렌더 스레드 분리를 채택하지 않은 근거와 재검토 조건은 [종료 시퀀스 — 종료 cascade + Drop tail](../architecture/shutdown-sequence.md).

`redraw.rs` 의 `SurfaceError` 분기(`Lost`/`Outdated` 재시도 등)는 전부 **호출이 반환됐을 때만** 도는 경로다. 반환하지 않는 상황은 그 분기가 다루지 못한다.

## 터미널 렌더 = 누적 후 1회 flush, 단일 패스 (필수 모델)

여러 터미널을 *각각 submit* 하지 않는다. 한 프레임의 모든 surface 인스턴스를 Vec 에 **누적**하고, 버퍼에 **한 번** 쓰고, **단일 render pass** 에서 surface 별 scissor + 인스턴스 range 로 그린다.

```rust
renderer.begin_frame();                       // ① accumulator 클리어
for surface in surfaces {                      // ② 각 surface 인스턴스를 Vec 에 push
    terminal.with_view(&display_viewport, |content| {
        renderer.append_terminal_viewport(&content, queue, &viewport, ansi, ...);
    });
}
renderer.flush_buffers(device, queue);         // ③ 누적분을 버퍼에 1회 write
// 단일 render pass 안에서:
renderer.render_all(&mut render_pass, w, h);   // ④ bg 패스 → glyph 패스, surface 별 scissor
```

### ① `begin_frame()`

per-frame accumulator(`bg_instances`, `glyph_instances`, `surface_ranges`)와 draw 카운터를 클리어하고 glyph atlas frame 카운터를 bump(per-page LRU stamp 일관성).

그 bump 는 LRU stamp 만이 아니라 **atlas의 프레임당 eviction 제한을 초기화한다** — atlas 는 한 프레임에 페이지를 한 번만 evict 하고, 그 제한을 푸는 것은 이 bump 뿐이다. 그래서 이 호출을 건너뛴 렌더 경로는 **다음 프레임으로 인식하지 못해 추가 eviction을 거절하고**, 증상은 glyph 가 조용히 rasterize 되지 않는 것으로만 나타난다. 그 상태 기계(`FrameClock`)는 device 없이 단독으로 검증된다(`cargo test -p tasty-font`).

`src/source_guards/frame_clock_arming.rs`는 `append_terminal_viewport`를 호출하는 함수에서
첫 append 전에 같은 수신자의 `begin_frame`을 루프 밖에서 호출하는지 검사한다.
`src/view/base.rs`에도 이름이 같은 함수가 있어 수신자까지 비교해야 한다.
이 소스 검사는 조건 분기, 호출 횟수, 프레임당 실행 횟수까지 확인하지 않는다.

### ② `append_terminal_viewport(...)`

한 surface 의 셀들을 `BgInstance`/`GlyphInstance` 로 만들어 accumulator Vec 에 push 한다. 동시에 그 surface 의 `(scissor rect, bg range, glyph range)` 를 `surface_ranges` 에 기록한다. **viewport offset 은 per-instance 로 각 인스턴스에 baked** 되므로(전역 uniform 을 surface 마다 다시 쓰지 않는다), surface 마다 uniform 갱신/submit 이 필요 없다. theme lock(`ansi` 팔레트)은 호출자가 **프레임당 1회** 잡아 넘긴다(surface 마다 잠그지 않음).

셀 하나는 bg 인스턴스 하나다. 칸이 긋는 선은 `renderer/cell_decor.rs` 한 곳에서 고르고, 현재 화면 경로(`fill_surface`)와 스크롤한 위치의 경로(`render_cell`)가 같은 `push_cell`로 낸다. 선 판단은 공백 칸 검사 앞이라 글자가 없는 칸도 선을 긋고, 넓은 글자는 왼쪽 칸 기준으로 판단해 두 칸에 긋는다(줄 끝에서는 한 칸). preedit가 덮는 칸은 긋지 않는다. 숨긴 칸(SGR 8)은 바탕만 내고 글리프와 프로그램 선을 내지 않는다(`draws_glyph`, `lines_for_cell`). 글리프 아래 선(밑줄)은 `BgInstance.line`(셀 위쪽에서 잰 위치와 높이)을 쓴 bg 인스턴스를 칸마다 하나 더하며, bg 패스가 글리프 패스보다 먼저라 글리프 아래에 놓인다. 글리프 위 선(취소선, 윗줄)은 그 칸의 글리프 다음에 글리프 인스턴스로 낸다. U+2588 아틀라스 항목의 가운데 한 점을 uv 크기 0으로 샘플해 칸 안 사각형을 채운다. 셰이더와 파이프라인, draw 수는 그대로다. 채운 사각형의 픽셀 값은 아직 재지 않았다. 지금 긋는 선은 포인터 아래 링크의 밑줄뿐이다. 프로그램 선(SGR 4, 9, 53)의 기하는 `LineGeometry.program`으로 받으며 값이 아직 없어 `None`이고, 그동안 인스턴스 수는 선이 없을 때와 같다. 강조(선택 → vi 커서 → 링크 → 검색)는 별도 인스턴스가 아니라 **그 셀의 bg 색을 바꾸고**(vi 커서 칸의 글자는 `vi_cursor_fg()`), bg 파이프라인은 `BlendState::REPLACE` 로 쓴다. 그래서 강조색의 alpha 는 GPU 가 아니라 CPU 에서 반영한다 — `renderer/overlay.rs` 의 `composite_over` 가 강조색을 그 셀의 현재 bg 위에 source-over 로 합성한다(불투명 강조색은 그대로 통과). 셀 기본 bg · 셀 속성 bg · Block 커서는 합성하지 않는다. 근거: [테마 시스템 (운영 상세)](../design/systems/theme.md).

### ③ `flush_buffers(device, queue)`

누적된 인스턴스를 `bg_instance_buffer`/`glyph_instance_buffer` 에 `write_buffer` 로 **한 번** 쓴다. 용량이 모자라면 ×2 로 키워 재할당(hard cap 16M 인스턴스 ≈ 1 GiB, 초과 시 clamp + warn).

### ④ `render_all(render_pass, w, h)`

단일 render pass 안에서 **bg 패스 전체 → glyph 패스 전체** 순으로 draw 한다. 각 패스는 `surface_ranges` 를 순회하며 surface 별 `set_scissor_rect(...)` 후 instanced `draw(0..6, range)`(6 vert = quad). 빈 range 는 skip.

> **왜 이 구조인가**: 예전엔 공유 버퍼를 offset 0 부터 덮어써서 *터미널마다 encoder+submit 을 분리* 해야 했다. 지금은 인스턴스를 누적하고 per-instance offset 을 baking 하므로, 버퍼 write 1회 + render pass 1개로 끝난다 — submit 폭증 없이 N 개 surface 를 그린다.

## 렌더러가 보는 것 — 입력은 인자로 오고, 이름은 크레이트다

셀 렌더러(`src/gfx/renderer.rs` + `src/gfx/renderer/`)는 앱 상태를 조회하지 않는다.
선택·vi 커서·링크 하이라이트·검색 강조·preedit·ANSI 팔레트는 전부 호출부
(`src/gfx/gpu/render_pass.rs`)가 만들어 인자로 넘긴다.

그 입력 타입을 **어느 이름으로 부르는가**도 계약의 일부다. 렌더러는 워크스페이스
크레이트를 직접 부른다 — 선택은 `tasty_selection`, 링크는 `tasty_terminal_link`,
폭 표는 `tasty_cell_width`, 글리프 아틀라스는 `tasty_font`, 사각형은 `tasty_model`.
본체에도 같은 것을 가리키는 재수출이 있지만(`crate::selection` =
`state::selection`, `crate::terminal_link` = `adapters::ui::terminal_link`) 렌더러는
그것을 쓰지 않는다. 거치면 타입이 이미 크레이트에 있는데도 **렌더러가 앱 상태와 UI
어댑터를 보는 모양이 표기에 남기** 때문이다. 근거·대안·재검토 조건은
[아키텍처 개요](../architecture/index.md).

그래서 렌더러가 부르는 본체 경로는 **`crate::cell_palette` 하나**다. 그것은 남겨 둔
것이다 — 셀 색 해석은 `gui` 게이트 밖에 있어야 하고(헤드리스 `debug.glyph_color`
핸들러가 같은 함수로 답한다), 함수를 복제하지 않으려면 둘이 같은 자리를 봐야 한다.

이 의존 규칙을 확인하는 자동 검사는 없다. 다음 명령으로 참조를 찾고 코드 문맥을 읽는다.

```bash
grep -rn 'crate::' src/gfx/renderer.rs src/gfx/renderer/ | grep -v '^[^:]*:[0-9]\+:[[:space:]]*//'
```

두 번째 필터는 줄 주석을 제외한다. `use crate::`만 검색하면 함수 본문에서 직접 쓰는
경로를 놓치므로 전체 `crate::`를 찾는다. 블록 주석·문자열은 결과에 남으므로 직접 구분한다.
반대로 `super::…`나 `use crate as c` 뒤의 `c::…`는 이 검색에 잡히지 않는다.
검색 결과가 적다는 것만으로 의존 규칙 준수를 확정하지 않는다.

`src/gfx/gpu*` 는 이 규칙의 대상이 아니다 — 그쪽은 `MainViewState`·`CoreState`·
`PluginManager` 를 받는 호스트 접착층이고, 본체 의존이 거짓이 아니라 사실이다. 다만
렌더 입력 타입 둘(`tasty_selection::*` · `tasty_terminal_link::LinkHighlight`)은
호출 양쪽에서 뜻이 하나로 남도록 그쪽 시그니처도 같은 이름을 쓴다.

## Surface configure 치수 clamp

`GpuState`(`src/gfx/gpu.rs`)는 `surface.configure` 에 넘기는 width/height 를 winit 경계에서 `device.limits().max_texture_dimension_2d`(어댑터별 실제 한계, 런타임 조회 — 하드코딩 금지)로 **clamp** 하고, 실제로 clamp 가 걸리면 `warn!` 을 남긴다. 상한을 넘는 치수가 오면 wgpu 가 panic 하기 때문이다(예: 외부 `SetWindowPos` 가 winit `Resized` 로 `1100×65535` 유입). 하한은 `1`(configure 는 0 불가), 0 은 최소화 신호로 `resize` early-return 이 configure 를 스킵한다. `resize` 와 `new`(startup) 모두 공통 `clamp_surface_dims(w,h,max)` 헬퍼를 쓴다.

**거부+안내 계층은 없다** — IPC/CLI/시작단/split 어디에도 상한 초과 치수를 주입하는 진입점이 없어(창 크기 변경 명령·저장된 geometry 복원 부재) winit OS 이벤트 경계가 유일한 유입 경로다. OS 이벤트는 거부할 수 없으므로 방어는 clamp + `warn!` 하나로 일원화한다. 정상 범위(≤max)에서는 no-op 이라 기존 동작에 영향이 없다.

### 중복 크기 통지와 강제 복구

`GpuState::resize`는 요청을 clamp한 물리 크기가 현재 크기와 같고 표면이 유효하면
주 표면과 오버레이의 configure를 생략한다. 요청이 어댑터 한계를 초과했으면
이 생략 판정 전에도 기존 clamp 경고를 남긴다. 실제 크기가 바뀌면 두 표면과 터미널
renderer 치수를 함께 갱신한다. 0 크기는 복구 중에도 configure하지 않는다.
`reconfigure`는 Lost/Outdated 복구를 위한 명시 호출이며 같은 크기도 재설정한다.
공용 acquire 경계도 Lost/Outdated를 기억하므로 부팅 오류·셸 설정 화면의 기존
resize-after-error 호출은 같은 크기를 전달해도 복구한다. 성공한 configure가 무효
상태를 해제한다.

이 생략은 GPU 치수 판단에만 적용한다. MainView의 배율 동기화·기본 grid·PTY resize와
오버레이 geometry 후처리를 크기 동등성 때문에 건너뛰지 않는다. 배율 변경 뒤 terminal
font signature는 다음 프레임에서 renderer 폰트와 uniform을 갱신한다. 설정·플러그인·
프리셋·종료 확인 창도 공용 resize를 사용한다.

`TASTY_LOG=tasty::gpu::surface=debug`는 주 표면의 configure/acquire/submit/present를
각각 반환 뒤 `elapsed_us`로 기록한다. submit은 같은 device queue의 오버레이·
offscreen 작업도 포함한다. configure 수는 시작 시 최초 설정을 제외한
재설정 호출 수이며, acquire 오류와 present 완료를 구분한다. 오버레이의 acquire와
present는 별도 표면 경로다. 완료 로그가 없다고 호출을 취소했거나 복구했다고 판단하지
않는다. 중복 configure 억제는 호출 빈도를 줄이며 남은 native 호출의 무상한 대기를
제거하지 않는다. GPU를 임의 스레드로 옮기거나 join을 포기하는 종료 정책은 도입하지
않는다.

## 주요 버퍼

| 버퍼 | 내용 |
|------|------|
| `uniform_buffer` | **전역** cell_size + viewport_size (윈도우 리사이즈 시만 갱신) |
| `bg_instance_buffer` | 셀 배경 인스턴스 (pos + bg_color), per-instance viewport offset baked |
| `glyph_instance_buffer` | 글리프 인스턴스 (pos + uv + fg_color + glyph_size), offset baked |

per-surface 위치 정보가 인스턴스에 들어가 있으므로 uniform 은 전역 1개로 충분하다.

## 프레임 구동 정책 — 무엇이 프레임을 유발하고 무엇이 상한을 거는가

프레임은 **`View::mark_dirty()` 하나로 수렴**한다. 이 호출은 `base.dirty`를 세우고 repaint gate가 허용하면 `Window::request_redraw()`를 요청한다. 지연된 요청은 dirty를 유지한다. 실제 렌더는 `render_if_dirty`(`src/view/main/redraw.rs`)가 `dirty` 일 때만 수행한다.

### 반환된 surface 오류의 재시도

MainView는 GPU 시도 직전에 dirty만 소비하고, `GpuState::render`가 실제 present를 마친
`Ok`에서 repaint의 성공 기준 시각을 갱신한다. acquire는 egui frame의 texture delta를
소비하기 전에 수행한다. 획득 실패로 이번 폰트·텍스처 갱신을 잃지 않도록 하기 위해서다.

Timeout과 그 밖의 비치명적 오류는 dirty를 복구하고 창별 `RepaintGate`에 단 하나의
지연 예약을 둔다. 첫 간격은 16ms이고 연속 실패마다 두 배로 늘려 최대 1초로 제한한다.
기존 about_to_wait의 deferred repaint/WaitUntil 경로가 깨우므로 입력·PTY 출력이
없어도 다음 시도가 진행된다. 재시도 전 stale redraw와 새 dirty 요청도 같은 대기
기한을 지킨다. 프레임 후 WebView 동기화의 dirty 재요청도 같은 기한을 지킨다.
WebView가 없는 창에서도 이 재요청 경로가 실행되므로 GPU 진입만 막으면 redraw 콜백이
계속 도는 문제가 남는다. 유발원별 정상 주사율 정책은 성공한 뒤 다시 적용된다.

Lost/Outdated는 먼저 강제 재구성한 뒤 같은 지연 재시도를 예약한다. OutOfMemory는
기존 error/crash-report 정책을 유지하고 자동 재시도 예약을 지운다. 종료 코드를 새로
정하지 않는다. 실제 present 성공은 실패 횟수와 대기 예약을 지우고, View 제거는 소유한
예약을 함께 버린다. 별도 worker나 해제할 반복 타이머가 없다. 반복 오류 로그와 crash
report에는 연속 실패 횟수 1, 2, 4, 8, …에서만 기록하여 경고 폭주를 막는다.

`TASTY_LOG=tasty::gpu::surface=debug`의 `phase="main_present"`는 MainView가 실제
present를 마친 프레임만 센다. 로딩 화면·모달의 present와 구별한다. 유휴 복구 검증은
초기 로딩 프레임 수나 캡처 요청 자체를 성공으로 해석하지 않고, 안정된 View에서 한 번
유발한 실패 뒤 추가 요청 없이 이 완료 로그와 예약했던 캡처 파일을 대조한다.

Debug 빌드의 opt-in 결함 주입은 `src/gfx/gpu/debug_surface.rs`에 격리한다.
`TASTY_DEBUG_GPU_FAULT=timeout:1`(또는 lost/outdated/oom)과
`TASTY_DEBUG_GPU_FAULT_AFTER_MS`는 첫 MainView GPU 프레임 이후 지정 시간이 지난
다음 acquire 시도를 합성 오류로 끝낸다. 창별 오류 횟수는 최대 1000회, 지연은 최대
60초이며 유휴 창을 스스로 깨우지 않는다. `TASTY_DEBUG_GPU_RESIZE_REPEATS`는 첫 시도에
동일 크기의 실제 공용 resize를 최대 1000회 호출하는 계측 fixture다. OS의 중복 이벤트를
발생시켰다는 뜻은 아니다. release에는 이 모듈과 환경 변수 처리 경로가 없다.

### 유발원과 상한 적용

상한은 `RepaintGate`(`src/view/repaint.rs`)가 `mark_dirty` 안에서 건다. 유발원은 `RepaintSource` 로 분류한다.

| 유발원 | `RepaintSource` | 상한 | 좌표 |
|---|---|---|---|
| 키·마우스·IME·리사이즈·포커스·구조 변경 | `Interactive` (기본값) | **정상 시 즉시 발화** | `mark_dirty()` 호출 전반 |
| PTY 출력 | `TerminalOutput` | 주사율까지 coalesce | `src/app/event_handler.rs` `handle_terminal_output` |
| egui 내부 delay 0 즉시 repaint | `EguiAnimation` | 주사율까지 coalesce | `AppEvent::EguiRepaint` 핸들러 |
| egui 지연 repaint(`request_repaint_after`) | `EguiAnimation` | 주사율까지 coalesce | `AppEvent::EguiRepaintAfter` → `Tick::EguiRepaint(창)` 발화 |
| attach mirror 갱신 | `AttachMirror` | 주사율까지 coalesce | `src/app/attach_poll.rs`, `src/app/attach_client.rs` |

egui 지연 repaint 는 repaint 콜백(`src/gfx/gpu.rs`)이 창마다 하나의 예약 시각만 유지해 보낸다. 아직 오지 않은 더 이른 예약이 있으면 같은 프레임에서 반복되는 요청을 보내지 않고, 더 이른 요청만 예약을 앞당긴다(`src/gfx/gpu/delayed_repaint.rs`). App 은 그 시각까지 남은 시간을 창별 일회성 타이머로 등록하고(`src/app/egui_repaint.rs`), 발화하면 즉시 요청과 같은 경로로 창을 다시 그린다. 그래서 툴팁 표시 지연, modifier-hint 표시 지연처럼 입력 없이 시간이 지나야 바뀌는 egui 화면이 제때 그려진다. 지연 요청이 없는 유휴 상태에서는 깨움이 늘지 않는다. UI 입력란 커서는 깜박이지 않는다. 호스트는 `tasty_egui_theme::apply_theme_to_egui` 가, 플러그인 egui-mesh 컨텍스트는 SDK 의 `EguiMeshCore` 가 `text_cursor.blink` 를 끈다. 깜박이면 포커스된 입력란이 있는 동안 0.5초마다 프레임을 그리고, 플러그인은 그때마다 호스트와 프레임을 주고받는다. 캐럿 모양 규칙은 [테마](../design/systems/theme.md#애니메이션과-스크롤)에 있다.

사용자 조작으로 발생한 요청을 즉시 처리하는 이유는 반응성이다 — 여기에 상한을 걸면 타이핑·클릭 지연이 그대로 늘어난다. 나머지는 주사율 안에서 합쳐 불필요한 프레임 요청을 줄인다.

### 상한값

**모니터가 보고하는 주사율**(`MonitorHandle::refresh_rate_millihertz()`)에서 최소 프레임 간격을 구한다. 5 초 TTL 로 재조회하므로 창을 다른 모니터로 옮기면 따라간다. 보고값이 없으면 60Hz 폴백, 상식 범위를 벗어나면 24~480Hz 로 clamp 한다. 고정 상수를 박지 않는 이유는 그 상수가 고주사율 환경에서 **오히려 상한**이 되기 때문이다.

상한 상태는 `ViewBase` 필드라 **창마다 독립**이다.

### 미뤄진 요청은 반드시 되살아난다

게이트는 `request_redraw()` 를 미룰 뿐 `dirty` 를 지우거나 요청을 버리지 않는다. 미뤄진 요청의 만기 시각은 `about_to_wait`(`drive_deferred_repaints`)이 `ControlFlow::WaitUntil` 로 재예약하며, 만기 tick 에서 `request_redraw()` 를 발화한다. 이 재예약이 유일한 복구 경로다 — 빠뜨리면 아무 이벤트도 오지 않는 순간에 그 프레임이 영영 오지 않는다.

`dirty` 를 **억제하지 않는** 것은 계약이다. `render_if_dirty` 의 doc 주석이 명시하듯 attach 서버의 원격 mirror 중계가 `dirty` 프레임에 종속돼 있어, 프레임을 없애면 원격 사용자 화면이 갱신되지 않는다. 상한은 요청 간격만 주사율에 맞추고 프레임 자체는 계속 흐르게 한다.

### 왜 present 층이 아니라 요청 층인가

`present_mode` 는 `Mailbox` 가 가능하면 `Mailbox`, 아니면 `Fifo` 다(`src/gfx/gpu.rs`, 선택 결과를 `info!` 로 남긴다). `Fifo` 로 고정하면 상한은 서지만 **모든** 리페인트에 최대 한 프레임의 present 블록이 실려 입력 반응성이 함께 나빠진다. 게다가 가상 디스플레이(xrdp 계열)에는 하드웨어 vblank 가 없어 `Fifo` 가 실제로 프레임을 묶어 준다는 보장도 없다. 요청 층에서 걸면 유발원별로 갈라 처리할 수 있어 반응성을 지키면서 초과 프레임만 없앤다.

원격 데스크톱 경유에서 이 상한이 특히 중요한 이유는 프레임당 비용이다. GPU 스캔아웃 경로가 없어 present 마다 GPU→CPU readback → X11 `PutImage` → 서버측 재인코딩을 타므로, 해당 경로에서는 프레임당 화면 전체(1920×1080×4B ≈ 8MB)의 전송 비용이 생긴다. 주사율을 넘겨 그린 프레임은 화면에 나타나지 못한 채 그 비용만 물고 버려진다.

## 성능 측정

GPU 렌더 파이프라인(단일 render pass batching + multi-page atlas — 이 문서 위 절들)이 surface 수·글리프 다양성 증가에 따라 *폭발하지 않는지* 를 실측하는 방법.

핵심 질문 둘: (1) draw call 수가 `O(surface)` 선형에 머무는가, (2) `terminals_ms` 가 surface 수에 따라 폭증하지 않는가.

### 측정 segment (`src/gfx/gpu.rs::render()`)

`Instant` 로 단계별 시간을, `CellRenderer` 카운터로 atlas/draw 통계를 잰다.

| 변수 | 의미 |
|------|------|
| `terminals_ms` | `begin_frame` + N×`append_terminal_viewport` + `flush_buffers` + `render_all`. 핵심 영역 |
| `gpu_total_ms` | `render()` 전체(layout + egui + tessellate + clear + terminals + egui_pass + present) |
| `draw_calls_total` | `RenderPass::draw()` 호출 수(`set_pipeline`/`set_scissor_rect` 별도; pipeline 스위치는 frame 당 2 고정) |
| `surfaces` | 활성(bg/glyph range 비어있지 않은) surface 수 |
| `atlas_evictions` | 누적 페이지 eviction(단조 증가 — *전후 차이* 가 의미) |
| `atlas_active_pages` / `atlas_entry_count_sum` | 마지막 sample 의 절대치(percentile 아님) |

### perf 로그 활성화

`src/gfx/perf.rs::PerfAggregator` 가 `WINDOW = 300` 프레임 ring buffer 를 모아 `DUMP_EVERY = 300` 마다 p50/p99/max 한 줄을 dump 한다.

- log target: `tasty::gfx::perf` (본체 기본 필터에서 비활성).
- 활성: `TASTY_LOG=tasty::gfx::perf=info` — 본체가 읽는 변수는 `TASTY_LOG` 다([crash-diagnostics](crash-diagnostics.md)).
- 한 줄: `perf n=300 surfaces=10 draws=20 terminals_ms p50=… p99=… max=… gpu_total_ms p50=… p99=… max=… atlas_evictions=… atlas_pages=… atlas_entries=…`

### 재현 시나리오 (`scripts/bench/`)

| 스크립트 | surface | 부하 | 핵심 관찰 |
|----------|---------|------|-----------|
| `perf-10-surfaces.sh` | 10 (split 9회) | 각 surface 에 `seq 1 5000 → echo` | `terminals_ms`/`draw_calls_total` 의 surface-scaling |
| `perf-cjk-atlas.sh` | 4 | CJK(한자/히라가나/한글) 대량 unique 코드포인트 | `atlas_evictions` delta + `atlas_pages` |
| `wasm-vs-process.sh` | — | WASM SDK vs 프로세스 plugin 비교 | plugin 런타임 오버헤드 |

- env: `PERF_DURATION_SECS`(기본 60), `PERF_PROFILE`(`release` 기본 / `dist` = full LTO, ~3.5× 빌드), `PERF_LOG_DIR`(perf 로그 위치, 기본 OS 임시 디렉토리 아래 `tasty-bench/` = `${TMPDIR:-/tmp}/tasty-bench`). `wasm-vs-process.sh` 의 csv 위치는 `BENCH_OUT_DIR`(같은 기본값). 회귀 측정은 동일 프로필 안에서.
- 스크립트는 `cargo run` 기동 → `tasty list info` 폴링으로 ready 대기 → `tasty split` 으로 surface 늘림 → 입력 주입 → perf 로그 마지막 샘플 추출. 단 두 perf 스크립트는 로그 필터를 `RUST_LOG` 로 넘기는데 본체는 `TASTY_LOG` 만 읽으므로, 지금 그대로는 perf 줄이 로그에 안 나온다. 출력 로그 경로는 각 스크립트 상단 `LOG_DIR` 참조.
- CJK 시나리오의 fallback 폰트 부재 사전 점검(`font fallback missing|no glyph for codepoint` grep → abort)은 무효다 — 그 문구와 `tasty::font` target 을 내는 코드가 없어 절대 abort 하지 않는다.

### 회귀 임계값

- `gpu_total_ms p99 > 30.0` → `SLOW_RENDER_MS`(`src/gfx/gpu.rs`) warn 트리거.
- `draw_calls_total > 2 × surfaces` → batching 회귀 가능성(perf 로그 `draws` vs `surfaces` 비교).

### 범위 밖

RSS/GPU 메모리 profile, flamegraph, scrollback viewport scroll 부하(release IPC 비노출이라 GUI viewport scroll 트리거 불가 — debug IPC 필요), `cargo bench` harness/CI 자동 임계 검사, Windows PowerShell 포트(스크립트는 bash 전제)는 본 측정 범위 밖.

## 진단

- `draw_call_count()` / `active_surface_count()` — 프레임의 bg/glyph draw 수, 활성 surface 수.
- 프레임 타이밍 계측은 `src/gfx/perf.rs` / redraw 경로. `tracing::warn!` 은 임계값 초과 시만.
- 셀 색이 의심되면 debug 의 `debug.glyph_color`(렌더러가 push 하는 실제 RGBA) — [debug-ipc.md](debug-ipc.md).
- **이벤트 루프 stall 워치독** (`crates/tasty-platform/src/stall_watchdog.rs`, 전 빌드) — 콜백 안의 5초 지연과, 처리할 IPC 명령이 큐에 남은 채 콜백 바깥에서 진행이 없는 5초 구간을 구별해 보고한다. 정상 idle과 의도적 대기는 제외한다. 콜백 바깥의 기준점은 마지막 콜백 진행과 연속 pending 시작 중 더 늦은 시각이다. 현재 단계와 나이가 있는 과거 GPU 단계를 구분하며 GPU/IME 원인을 확정하거나 복구하지 않는다. 첫 hang 파일과 30초 재보고 제한을 유지하고 main-loop probe를 추가하지 않는다. 필드·한계는 [크래시 진단](crash-diagnostics.md)을 따른다. debug 재현은 `tasty debug gpu-stall --ms N`이다([debug-ipc.md](debug-ipc.md)).
  - **메인 스레드를 의도적으로 막는 동기 호출은 `stall_watchdog::without_stall_watch(..)` 로 감싼다.** 두 번째 실행 실패를 알리는 OS `MessageDialog`처럼 "돌아오지 않는 것이 정상" 인 구간을 감싸지 않으면, 사용자가 선택에 5 초만 써도 `crash-reports/` 에 오탐 리포트가 남아 그 디렉토리가 신호를 잃는다.
- 리페인트 유발원별 요청 수 / 지연 수 / 실제 present 수는 1 초 창 dump: `TASTY_LOG=tasty::view::repaint=info`(dev 빌드는 `~/.tasty-debug/debug-dev.log` 에 그냥 남는다). 요청이 많아도 실제 렌더가 합쳐졌는지를 이 한 줄로 구분한다.
