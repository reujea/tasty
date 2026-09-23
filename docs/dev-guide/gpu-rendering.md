# GPU 렌더링 구조

터미널은 wgpu 커스텀 셰이더 파이프라인으로 그린다 — egui UI 와 별도로, 각 셀의 배경 + 글리프를 GPU 인스턴스 렌더링한다. 핵심 렌더러는 `src/gfx/renderer.rs` 의 `CellRenderer`.

## 프레임 흐름

1. **Clear pass** — 배경색으로 클리어
2. **Terminal pass** — 아래 4단계 accumulator 로 모든 터미널을 한 번에
3. **egui-mesh pass** — plugin 이 자기 프로세스에서 tessellate 한 mesh 를 surface 영역에 합성 (host chrome 아래 layer). 채널 상세는 [egui-mesh-channel](egui-mesh-channel.md)
4. **egui pass** — 사이드바·탭바·팝업 등 UI 오버레이

## 렌더는 이벤트 루프 스레드에서 돈다 (제약)

winit `ApplicationHandler` 콜백은 전부 **하나의 스레드**에서 동기 실행되고, GPU 렌더는 그중 `WindowEvent::RedrawRequested` 처리 안에서 돈다. IPC 도 같은 스레드다 — IPC 서버 스레드는 요청을 메인 스레드로 넘기고 응답을 기다린다(`TcpIpcServer::dispatch_and_await` → `AppEvent::IpcReady` → `process_ipc`).

따라서 **GPU 호출 하나가 반환하지 않으면 키·마우스·IPC 가 전부 함께 멎는다.** 이건 렌더만 느려지는 게 아니라 창이 통째로 무응답이 되는 것이고, panic 이 아니므로 crash report 도 남지 않는다.

wgpu 24 기준으로 호출별 상한은 다음과 같다:

| 호출 | 상한 |
|------|------|
| `surface.get_current_texture()` | **있음** — wgpu-core 가 1 초(`FRAME_TIMEOUT_MS`)를 걸고 초과 시 `SurfaceError::Timeout` 반환 |
| `queue.submit(...)` | **없음** |
| `SurfaceTexture::present()` | **없음** |

상한이 없는 두 호출에는 wgpu 가 취소·타임아웃 API 를 제공하지 않는다. 그래서 tasty 는 이 상황을 *막지* 못하고 **관측만** 한다 — 아래 "진단" 의 stall 워치독. 렌더 스레드 분리를 채택하지 않은 근거와 재검토 조건은 [ADR-0091](../adr/0091-render-stall-watchdog-observation-only.md).

`redraw.rs` 의 `SurfaceError` 분기(`Lost`/`Outdated` 재시도 등)는 전부 **호출이 반환됐을 때만** 도는 경로다. 반환하지 않는 상황은 그 분기가 다루지 못한다.

## 터미널 렌더 = 누적 후 1회 flush, 단일 패스 (필수 모델)

여러 터미널을 *각각 submit* 하지 않는다. 한 프레임의 모든 surface 인스턴스를 Vec 에 **누적**하고, 버퍼에 **한 번** 쓰고, **단일 render pass** 에서 surface 별 scissor + 인스턴스 range 로 그린다.

```rust
renderer.begin_frame();                       // ① accumulator 클리어
for surface in surfaces {                      // ② 각 surface 인스턴스를 Vec 에 push
    renderer.append_terminal_viewport(term, queue, &viewport, ansi, ...);
}
renderer.flush_buffers(device, queue);         // ③ 누적분을 버퍼에 1회 write
// 단일 render pass 안에서:
renderer.render_all(&mut render_pass, w, h);   // ④ bg 패스 → glyph 패스, surface 별 scissor
```

### ① `begin_frame()`

per-frame accumulator(`bg_instances`, `glyph_instances`, `surface_ranges`)와 draw 카운터를 클리어하고 glyph atlas frame 카운터를 bump(per-page LRU stamp 일관성).

그 bump 는 LRU stamp 만이 아니라 **atlas 의 eviction 스로틀을 다시 무장한다** — atlas 는 한 프레임에 페이지를 한 번만 evict 하고, 그 제한을 푸는 것은 이 bump 뿐이다. 그래서 이 호출을 건너뛴 렌더 경로는 **한 번 evict 한 뒤 이후의 모든 eviction 을 영구히 거절하고**, 증상은 glyph 가 조용히 rasterize 되지 않는 것으로만 나타난다. 그 상태 기계(`FrameClock`)는 device 없이 단독으로 검증된다(`cargo test -p tasty-font`).

그 시계를 **실제로 감는지**는 이쪽 크레이트의 소스에 대한 물음이라 거기서 못 묻는다 — `src/source_guards/frame_clock_arming.rs` 가 판정한다: `append_terminal_viewport` 를 부르는 모든 함수가 첫 append 보다 앞에서, 어떤 루프에도 안 들어간 자리에서, **그 append 와 같은 수신자 위의** `begin_frame` 을 부르는가. 수신자를 함께 보는 이유는 본체에 동명이인이 있기 때문이다 — `src/view/base.rs` 의 뷰 계층 `begin_frame` 은 이름이 같지만 atlas 시계와 무관하다. 정적 판정이라 **조건 분기 · 호출 횟수 · 한 프레임에 그 함수가 몇 번 불리는가는 안 본다** — 그 가드의 모듈 주석에 못 보는 갈래가 전부 적혀 있다.

### ② `append_terminal_viewport(...)`

한 surface 의 셀들을 `BgInstance`/`GlyphInstance` 로 만들어 accumulator Vec 에 push 한다. 동시에 그 surface 의 `(scissor rect, bg range, glyph range)` 를 `surface_ranges` 에 기록한다. **viewport offset 은 per-instance 로 각 인스턴스에 baked** 되므로(전역 uniform 을 surface 마다 다시 쓰지 않는다), surface 마다 uniform 갱신/submit 이 필요 없다. theme lock(`ansi` 팔레트)은 호출자가 **프레임당 1회** 잡아 넘긴다(surface 마다 잠그지 않음).

셀 하나는 bg 인스턴스 하나다. 강조(선택 → vi 커서 → 링크 → 검색)는 별도 인스턴스가 아니라 **그 셀의 bg 색을 바꾸고**, bg 파이프라인은 `BlendState::REPLACE` 로 쓴다. 그래서 강조색의 alpha 는 GPU 가 아니라 CPU 에서 반영한다 — `renderer/overlay.rs` 의 `composite_over` 가 강조색을 그 셀의 현재 bg 위에 source-over 로 합성한다(불투명 강조색은 그대로 통과). 셀 기본 bg · 셀 속성 bg · Block 커서는 합성하지 않는다. 근거: [ADR-0460](../adr/0460-cell-highlight-alpha-is-composited-on-the-cpu-over-the-cell-bg.md).

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
[ADR-0342](../adr/0342-the-cell-renderer-names-the-crates-not-the-host-re-exports.md).

그래서 렌더러가 부르는 본체 경로는 **`crate::cell_palette` 하나**다. 그것은 남겨 둔
것이다 — 셀 색 해석은 `gui` 게이트 밖에 있어야 하고(헤드리스 `debug.glyph_color`
핸들러가 같은 함수로 답한다), 함수를 복제하지 않으려면 둘이 같은 자리를 봐야 한다.

재는 법 — 이 사실에 자동 채널은 없다. 새 `use crate::…` 한 줄은 컴파일되고 어떤
시험도 안 깬다.

```bash
grep -rn 'crate::' src/gfx/renderer.rs src/gfx/renderer/ | grep -v '^[^:]*:[0-9]\+:[[:space:]]*//'
# 두 줄 — 둘 다 cell_palette 다
```

**두 단계인 이유가 있다.** 좌변이 `.rs` 원문이므로 앞 명령만 쓰면 **이 규칙을 적은
`renderer.rs` 머리 주석의 산문까지 센다** — 상시 잡음 둘이 섞이면 나중의 회귀 한 줄과
안 갈린다. 뒤의 `grep -v` 가 내용이 `//` 로 시작하는 줄을 버린다.

좌변을 `use crate::` 로 좁히는 쪽이 간단해 보이지만 **그건 틀린다.** 이 경계가 실제로
지운 형태 중 둘이 `use` 없이 본문에 박힌 `crate::selection::is_selected(...)` 였다 —
좁힌 좌변은 그 형태를 못 본다. 실측(base `63a777ecc`, 좌변은 위 명령 그대로 —
`renderer.rs` + `renderer/` 아래 **다섯 파일** 전수): `use crate::` 는 10 줄, 위 두
단계는 12 줄이고 **차이가 정확히 그 두 자리**다. 반대로 위 두 단계는 블록 주석·문자열
리터럴 안의 `crate::` 를 그대로 세는데, 그건 더 많이 잡는 방향이라 눈으로 한 번 갈라
읽으면 된다.

**이 재는 법이 못 보는 것** — "조용한 통과가 없다" 고는 말할 수 없다. 두 갈래다.

- **닫은 것**: `grep -v` 의 패턴을 줄머리에 고정했다(`^[^:]*:[0-9]\+:` 뒤에서만 `//` 를
  본다). 고정 전에는 그 패턴이 줄 어디에서든 맞아, 진짜 코드 줄이라도 후행 주석에
  `파일:줄:` 인용이 들어 있으면 통째로 버려졌다. 탐침
  `pub const _P: bool = crate::state::FLAG; // see <파일>:99: // gui gate` 가
  고정 전 필터 뒤 **0**, 고정 뒤 **남는다**. 덤으로 탭으로 들여쓴 줄 주석도 이제 버려진다
  (옛 패턴의 ` *//` 는 탭을 안 먹었다).
- **못 닫는 것**: `crate::` 라는 **글자가 없는** 본체 의존은 어떤 텍스트 술어로도 안 보인다.
  탐침 둘 다 필터 뒤 목록에 안 나온다 — `use super::super::super::state::AppState;` 와
  `use crate as c;` + `c::state::AppState`. 이건 grep 의 한계라 리뷰가 봐야 한다.

**이 두 수를 다시 잴 때 좌변을 줄이지 마라.** `renderer.rs` 와 `line_render.rs` 둘만
꺼내 세면 `renderer/pipeline.rs` 의 `use crate::font::…` 한 줄이 빠져 10·12 가 9·11 로
나온다. 차이 2 는 그대로라 결론이 안 흔들리고, 그래서 **틀린 절대값이 안 들킨다.**

`src/gfx/gpu*` 는 이 규칙의 대상이 아니다 — 그쪽은 `AppState`·`CoreState`·
`PluginManager` 를 받는 호스트 접착층이고, 본체 의존이 거짓이 아니라 사실이다. 다만
렌더 입력 타입 둘(`tasty_selection::*` · `tasty_terminal_link::LinkHighlight`)은
호출 양쪽에서 뜻이 하나로 남도록 그쪽 시그니처도 같은 이름을 쓴다.

## Surface configure 치수 clamp

`GpuState`(`src/gfx/gpu.rs`)는 `surface.configure` 에 넘기는 width/height 를 winit 경계에서 `device.limits().max_texture_dimension_2d`(어댑터별 실제 한계, 런타임 조회 — 하드코딩 금지)로 **clamp** 하고, 실제로 clamp 가 걸리면 `warn!` 을 남긴다. 상한을 넘는 치수가 오면 wgpu 가 panic 하기 때문이다(예: 외부 `SetWindowPos` 가 winit `Resized` 로 `1100×65535` 유입). 하한은 `1`(configure 는 0 불가), 0 은 최소화 신호로 `resize` early-return 이 configure 를 스킵한다. `resize` 와 `new`(startup) 모두 공통 `clamp_surface_dims(w,h,max)` 헬퍼를 쓴다.

**거부+안내 계층은 없다** — IPC/CLI/시작단/split 어디에도 상한 초과 치수를 주입하는 진입점이 없어(창 크기 변경 명령·저장된 geometry 복원 부재) winit OS 이벤트 경계가 유일한 유입 경로다. OS 이벤트는 거부할 수 없으므로 방어는 clamp + `warn!` 하나로 일원화한다. 정상 범위(≤max)에서는 no-op 이라 기존 동작에 영향이 없다.

## 주요 버퍼

| 버퍼 | 내용 |
|------|------|
| `uniform_buffer` | **전역** cell_size + viewport_size (윈도우 리사이즈 시만 갱신) |
| `bg_instance_buffer` | 셀 배경 인스턴스 (pos + bg_color), per-instance viewport offset baked |
| `glyph_instance_buffer` | 글리프 인스턴스 (pos + uv + fg_color + glyph_size), offset baked |

per-surface 위치 정보가 인스턴스에 들어가 있으므로 uniform 은 전역 1개로 충분하다.

## 프레임 구동 정책 — 무엇이 프레임을 유발하고 무엇이 상한을 거는가

프레임은 **`View::mark_dirty()` 하나로 수렴**한다. 이 호출이 `base.dirty` 를 세우고 그 자리에서 `Window::request_redraw()` 를 발화하므로, `mark_dirty()` 호출 = 프레임 1 회 요청이다. 실제 렌더는 `render_if_dirty`(`src/view/main/redraw.rs`)가 `dirty` 일 때만 수행한다.

### 유발원과 상한 적용

상한은 `RepaintGate`(`src/view/repaint.rs`)가 `mark_dirty` 안에서 건다. 유발원은 `RepaintSource` 로 분류한다.

| 유발원 | `RepaintSource` | 상한 | 좌표 |
|---|---|---|---|
| 키·마우스·IME·리사이즈·포커스·구조 변경 | `Interactive` (기본값) | **없음 — 즉시 발화** | `mark_dirty()` 호출 전반 |
| PTY 출력 | `TerminalOutput` | 주사율까지 coalesce | `src/app/event_handler.rs` `handle_terminal_output` |
| egui 내부 delay 0 즉시 repaint | `EguiAnimation` | 주사율까지 coalesce | `AppEvent::EguiRepaint` 핸들러 |
| attach mirror 갱신 | `AttachMirror` | 주사율까지 coalesce | `src/app/attach_poll.rs`, `src/app/attach_client.rs` |

사용자 조작발을 통과시키는 이유는 반응성이다 — 여기에 상한을 걸면 타이핑·클릭 지연이 그대로 늘어난다. 나머지는 사람이 개별 프레임을 구분하지 못하므로 묶어도 체감이 없다.

### 상한값

**모니터가 보고하는 주사율**(`MonitorHandle::refresh_rate_millihertz()`)에서 최소 프레임 간격을 구한다. 5 초 TTL 로 재조회하므로 창을 다른 모니터로 옮기면 따라간다. 보고값이 없으면 60Hz 폴백, 상식 범위를 벗어나면 24~480Hz 로 clamp 한다. 고정 상수를 박지 않는 이유는 그 상수가 고주사율 환경에서 **오히려 상한**이 되기 때문이다.

상한 상태는 `ViewBase` 필드라 **창마다 독립**이다.

### 미뤄진 요청은 반드시 되살아난다

게이트는 `request_redraw()` 를 미룰 뿐 `dirty` 를 지우거나 요청을 버리지 않는다. 미뤄진 요청의 만기 시각은 `about_to_wait`(`drive_deferred_repaints`)이 `ControlFlow::WaitUntil` 로 재예약하며, 만기 tick 에서 `request_redraw()` 를 발화한다. 이 재예약이 유일한 복구 경로다 — 빠뜨리면 아무 이벤트도 오지 않는 순간에 그 프레임이 영영 오지 않는다.

`dirty` 를 **억제하지 않는** 것은 계약이다. `render_if_dirty` 의 doc 주석이 명시하듯 attach 서버의 원격 mirror 중계가 `dirty` 프레임에 종속돼 있어, 프레임을 없애면 원격 사용자 화면이 굶는다. 상한은 cadence 만 주사율에 맞추고 프레임 자체는 계속 흐르게 한다.

### 왜 present 층이 아니라 요청 층인가

`present_mode` 는 `Mailbox` 가 가능하면 `Mailbox`, 아니면 `Fifo` 다(`src/gfx/gpu.rs`, 선택 결과를 `info!` 로 남긴다). `Fifo` 로 고정하면 상한은 서지만 **모든** 리페인트에 최대 한 프레임의 present 블록이 실려 입력 반응성이 함께 나빠진다. 게다가 가상 디스플레이(xrdp 계열)에는 하드웨어 vblank 가 없어 `Fifo` 가 실제로 프레임을 묶어 준다는 보장도 없다. 요청 층에서 걸면 유발원별로 갈라 처리할 수 있어 반응성을 지키면서 초과 프레임만 없앤다.

원격 데스크톱 경유에서 이 상한이 특히 중요한 이유는 프레임당 비용이다. GPU 스캔아웃 경로가 없어 present 마다 GPU→CPU readback → X11 `PutImage` → 서버측 재인코딩을 타므로, 프레임당 화면 전체(1920×1080×4B ≈ 8MB)가 소켓으로 흐른다. 주사율을 넘겨 그린 프레임은 화면에 나타나지 못한 채 그 비용만 물고 버려진다.

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
- **이벤트 루프 stall 워치독** (`crates/tasty-platform/src/stall_watchdog.rs`, 전 빌드) — winit 콜백이 5 초 안에 반환하지 않으면 `tracing::error!(target: "tasty::stall")` 과 `~/.tasty/crash-reports/hang-<ts>.log` 에 "어느 콜백(`redraw` 등)의 어느 GPU 단계(`acquire`/`submit`/`present`)에서 몇 ms 째 멎었는가"를 남긴다. 같은 stall 은 30 초마다 재보고하되 파일은 stall 당 1 개다. 복구는 하지 않는다. 재현은 debug 전용 `tasty debug gpu-stall --ms N` — 다음 프레임의 `present` 직전을 1 회 블로킹한다([debug-ipc.md](debug-ipc.md)).
  - **메인 스레드를 의도적으로 막는 동기 호출은 `stall_watchdog::without_stall_watch(..)` 로 감싼다.** native 파일 다이얼로그(`rfd`)처럼 "돌아오지 않는 것이 정상" 인 구간을 감싸지 않으면, 사용자가 선택에 5 초만 써도 `crash-reports/` 에 오탐 리포트가 남아 그 디렉토리가 신호를 잃는다.
- 리페인트 유발원별 요청 수 / 지연 수 / 실제 present 수는 1 초 창 dump: `TASTY_LOG=tasty::view::repaint=info`(dev 빌드는 `~/.tasty-debug/debug-dev.log` 에 그냥 남는다). 요청이 많아도 실제 렌더가 합쳐졌는지를 이 한 줄로 구분한다.
