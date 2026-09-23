# GPU 성능 측정

GPU 렌더 파이프라인(단일 render pass batching + multi-page atlas — [gpu-rendering](gpu-rendering.md))이 surface 수·글리프 다양성 증가에 따라 *폭발하지 않는지* 를 실측하는 방법.

핵심 질문 둘: (1) draw call 수가 `O(surface)` 선형에 머무는가, (2) `terminals_ms` 가 surface 수에 따라 폭증하지 않는가.

## 측정 segment (`src/gfx/gpu.rs::render()`)

`Instant` 로 단계별 시간을, `CellRenderer` 카운터로 atlas/draw 통계를 잰다.

| 변수 | 의미 |
|------|------|
| `terminals_ms` | `begin_frame` + N×`append_terminal_viewport` + `flush_buffers` + `render_all`. 핵심 영역 |
| `gpu_total_ms` | `render()` 전체(layout + egui + tessellate + clear + terminals + egui_pass + present) |
| `draw_calls_total` | `RenderPass::draw()` 호출 수(`set_pipeline`/`set_scissor_rect` 별도; pipeline 스위치는 frame 당 2 고정) |
| `surfaces` | 활성(bg/glyph range 비어있지 않은) surface 수 |
| `atlas_evictions` | 누적 페이지 eviction(단조 증가 — *전후 차이* 가 의미) |
| `atlas_active_pages` / `atlas_entry_count_sum` | 마지막 sample 의 절대치(percentile 아님) |

## perf 로그 활성화

`src/gfx/perf.rs::PerfAggregator` 가 `WINDOW = 300` 프레임 ring buffer 를 모아 `DUMP_EVERY = 300` 마다 p50/p99/max 한 줄을 dump 한다.

- log target: `tasty::gfx::perf` (본체 기본 필터에서 비활성).
- 활성: `TASTY_LOG=tasty::gfx::perf=info` — 본체가 읽는 변수는 `TASTY_LOG` 다([crash-diagnostics](crash-diagnostics.md)).
- 한 줄: `perf n=300 surfaces=10 draws=20 terminals_ms p50=… p99=… max=… gpu_total_ms p50=… p99=… max=… atlas_evictions=… atlas_pages=… atlas_entries=…`

## 재현 시나리오 (`scripts/bench/`)

| 스크립트 | surface | 부하 | 핵심 관찰 |
|----------|---------|------|-----------|
| `perf-10-surfaces.sh` | 10 (split 9회) | 각 surface 에 `seq 1 5000 → echo` | `terminals_ms`/`draw_calls_total` 의 surface-scaling |
| `perf-cjk-atlas.sh` | 4 | CJK(한자/히라가나/한글) 대량 unique 코드포인트 | `atlas_evictions` delta + `atlas_pages` |
| `wasm-vs-process.sh` | — | WASM SDK vs 프로세스 plugin 비교 | plugin 런타임 오버헤드 |

- env: `PERF_DURATION_SECS`(기본 60), `PERF_PROFILE`(`release` 기본 / `dist` = full LTO, ~3.5× 빌드), `PERF_LOG_DIR`(perf 로그 위치, 기본 OS 임시 디렉토리 아래 `tasty-bench/` = `${TMPDIR:-/tmp}/tasty-bench`). `wasm-vs-process.sh` 의 csv 위치는 `BENCH_OUT_DIR`(같은 기본값). 회귀 측정은 동일 프로필 안에서.
- 스크립트는 `cargo run` 기동 → `tasty list info` 폴링으로 ready 대기 → `tasty split` 으로 surface 늘림 → 입력 주입 → perf 로그 마지막 샘플 추출. 단 두 perf 스크립트는 로그 필터를 `RUST_LOG` 로 넘기는데 본체는 `TASTY_LOG` 만 읽으므로, 지금 그대로는 perf 줄이 로그에 안 나온다. 출력 로그 경로는 각 스크립트 상단 `LOG_DIR` 참조.
- CJK 시나리오의 fallback 폰트 부재 사전 점검(`font fallback missing|no glyph for codepoint` grep → abort)은 무효다 — 그 문구와 `tasty::font` target 을 내는 코드가 없어 절대 abort 하지 않는다.

## 회귀 임계값

- `gpu_total_ms p99 > 30.0` → `SLOW_RENDER_MS`(`src/gfx/gpu.rs`) warn 트리거.
- `draw_calls_total > 2 × surfaces` → batching 회귀 가능성(perf 로그 `draws` vs `surfaces` 비교).

## 범위 밖

RSS/GPU 메모리 profile, flamegraph, scrollback viewport scroll 부하(release IPC 비노출이라 GUI viewport scroll 트리거 불가 — debug IPC 필요), `cargo bench` harness/CI 자동 임계 검사, Windows PowerShell 포트(스크립트는 bash 전제)는 본 측정 범위 밖.
