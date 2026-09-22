# 메모리 누수 soak 테스트

메모리 누수를 확인하는 방법을 정리한다. CI 대상이 아니다 — 필요할 때(릴리스 전, 의심 증상, 정기 점검) 수동으로 돌리는 장시간 검증이며, 하루 단위 실행을 전제로 설계됐다.

핵심 구조는 **탐지와 원인규명의 2단계 분리**다:

1. **탐지 (soak 하네스)** — 싸고 크로스플랫폼. 반복 워크로드를 돌리며 지표를 기록하고, 분석기가 "어느 시나리오에서 무엇이 새는가"를 국소화한다.
2. **원인규명 (attribution)** — 플랫폼 전용 도구로 특정된 시나리오만 재실행해 할당 스택을 얻는다.

## 누수의 4계층

계층마다 판정 기준과 도구가 다르다. 하나의 도구로 전부 볼 수 없다.

| 계층 | 예 | 판정 | 1차 도구 |
|------|----|------|---------|
| L1 진성 heap 누수 (unreachable) | `Rc` 순환, `mem::forget` | 종료 시 도달불가 블록 = 0 | LSAN(Linux) / `leaks`(macOS) |
| L2 도달 가능하지만 무한 성장 | 맵/캐시 항목 미제거, retain 누락 | 사이클당 RSS 기울기 ≈ 0 | soak + heaptrack/Instruments/UMDH |
| L3 GPU 리소스 | wgpu 텍스처/버퍼 미해제, `egui_mesh_targets` 잔류 | 카운트 기준선 복귀 (정수 엄격) | `system.gpu_stats` — **OS 도구로는 불가시** |
| L4 OS 핸들·프로세스 | ConPTY 핸들, conhost/셸 좀비 (ADR-0034) | 핸들·자식 수 기준선 복귀 | soak 외부 측정 |

장기 실행 앱의 실전 누수 대부분은 **L2** 다. LSAN/valgrind 는 "종료 시 unreachable" 만 누수로 보므로 L2 를 통과시킨다 — 그래서 시간축 diff(soak)가 주력이고 exit-time 판정은 보조다.

## 1단계 — soak 하네스 실행

`tests/soak_memory.rs` (`#[ignore]`, CI 에서 안 돎). `TastyInstance` 로 격리 인스턴스(`TASTY_HOME` 격리)를 띄워 시나리오를 반복하고 체크포인트마다 JSONL 로 기록한다.

```bash
# 스모크 (10분)
SOAK_SCENARIO=s9 SOAK_DURATION_SECS=600 \
  cargo test --release --test soak_memory -- --ignored --nocapture

# 본 실행 (24시간, release + 심볼 — attribution 대비)
CARGO_PROFILE_RELEASE_DEBUG=1 SOAK_SCENARIO=s9 SOAK_DURATION_SECS=86400 \
  cargo test --release --test soak_memory -- --ignored --nocapture
```

**release 빌드를 권장한다.** debug 빌드는 ① `~/.tasty-debug/debug-dev.log` 파일 레이어가 debug 레벨 고정·무로테이션이라 24h 에 수 GB 가 쌓이고 ② attribution 도구도 release+심볼 조합을 원한다. 심볼은 `CARGO_PROFILE_RELEASE_DEBUG=1` 로 Cargo.toml 수정 없이 켠다.

env 제어:

| env | 의미 | 기본 |
|-----|------|------|
| `SOAK_SCENARIO` | 아래 시나리오 표 | `s9` |
| `SOAK_DURATION_SECS` | 실행 시간 | 600 |
| `SOAK_CYCLES` | 사이클 수 상한 | 무제한 |
| `SOAK_CHECKPOINT_EVERY` | 체크포인트 간격(사이클) | 10 |
| `SOAK_OUT_DIR` | JSONL 출력 위치 | OS 임시 디렉토리 아래 `tasty-soak/` (`std::env::temp_dir()` — `$TMPDIR`/`/tmp` 또는 `%TEMP%`) |

하네스는 시작 시 `soak: … out=<경로>` 로 실제 출력 파일 경로를 stdout 에 찍는다.

### 시나리오 (직교 분해)

탐지 1차는 `s9`(혼합), FLAG 가 뜨면 개별 시나리오로 국소화한다.

| ID | 워크로드 | 주 감시 대상 |
|----|---------|-------------|
| `s1` | tab 생성→준비 대기→닫기 | ConPTY/셸 수명(L4), surface 정리(L2·L3) |
| `s2` | surface split→닫기 | per-surface GPU, 레이아웃 트리 |
| `s4` | 대량 출력(스크롤백 상한 미만) | 링버퍼, VTE 파서, glyph atlas |
| `s6` | explorer · image · markdown 을 사이클마다 돌아가며 **`surface0` 의 surface 분할로** 열고 닫는다(`split` → `surface.close`) | explorer: 호스트 view store(`drop_view`) · image: plugin egui-mesh(`egui_mesh_targets` retain) · markdown: plugin webview(plugin 쪽 per-surface 상태, host shared buffer 매핑) |
| `s7` | 조회 IPC 연타(매 호출 새 TCP 연결) | per-conn 상태, telemetry 버킷 |
| `s8` | idle | 타이머/폴링 바닥 드리프트 |
| `s9` | s1~s7 결정적 가중 혼합 | 종합 회귀 |

`s6` 이 새 탭(`tab.create`)이 아니라 분할로 여는 이유: 에이전트가 만든 탭은 선택되지 않아 렌더되지 않고([ADR-0502](../adr/0502-an-agent-created-tab-does-not-take-the-users-tab.md)), release 에는 탭을 고르는 API 가 없다(원칙 3) — 그렇게 열면 view store · egui-mesh 경로를 안 탄 채 초록이 난다. 분할한 surface 는 `surface0` 이 든 탭, 곧 보이는 탭 안에 서므로 곧바로 렌더된다. debug 전용 `debug.switch_tab` 으로 탭을 고르는 방법은 soak 을 debug 프로필에 묶어 수치의 의미가 바뀌므로 쓰지 않는다. 새 워크스페이스(`workspace.create`)로 여는 것도 같은 이유로 안 된다 — 워크스페이스를 고르는 `workspace.select` 가 release 에 없어 보이지 않는 워크스페이스 안에 선다. 이 설계는 ADR-0502 가 바뀌거나 호스트가 webview 렌더를 셀 수 있게 되면 다시 본다.

**`s6` 은 경로를 탔는지 열어 둔 채로 확인한다.** 닫은 뒤의 체크포인트는 경로를 안 탔을 때도 기준선(0)이라 그 값으로는 못 가른다. 그래서 하네스는 explorer 를 열면 `system.gpu_stats` 의 `explorer_views` 가, image 를 열면 `egui_mesh_targets` 가 연 직전보다 커질 때까지 기다리고, 30 초 안에 안 커지면 **soak 을 실패시킨다**(`… was never rendered … the scenario is not measuring its path`). markdown(webview)은 호스트가 세는 값이 없어 이 확인이 없다 — 고정 700 ms 뒤에 닫는다. 분할이 `surface0` 을 매번 리사이즈하므로 `s6` 에는 `s2` 의 레이아웃·PTY 리사이즈 성분이 함께 섞인다. `s9` 의 s6 몫(8 사이클마다 한 번)도 같은 함수를 부르고 세 view 를 차례로 돈다.

미구현: workspace 삭제 churn(삭제 release IPC 없음 — list/create/update/move 뿐), 창 resize churn(resize 는 사용자 조작이라 release IPC 없음, 원칙 1).

### 기록 지표

- **외부** (하네스가 sysinfo 로 측정, 관찰자 효과 0): 프로세스 **트리 합산** RSS, root RSS, 이름별 자식 수·**이름별 자식 RSS 합산**(`children_rss_by_name` — 누수 프로세스 1차 특정), 핸들(Windows)/fd(Linux/macOS) 수.
- **내부** (`system.gpu_stats` IPC · CLI `tasty list gpu-stats`): wgpu 전역 리포트(buffers/textures/texture_views/bind_groups 등 live 카운트), 창별 `egui_mesh_targets`/`_popup_targets`/`_banner_targets` len, atlas(eviction/pages/entries), draw calls, 창별 호스트 explorer view 수(`explorer_views` — main 이 아닌 창은 `null`).
- **부수 신호**: `input_incidents` — split close 직후 `surface.send` 첫 바이트 유실 레이스의 발생 횟수(하네스가 감지·재시도하며 계수).

## 2단계 — 판정

```bash
python scripts/soak/analyze.py "${TMPDIR:-/tmp}/tasty-soak/soak-s9-<ts>.jsonl" [--plot out.png]
```

warmup(기본 앞 10% 체크포인트)을 제외하고:

- **L2**: 트리/root RSS 에 OLS. `기울기 > 1KB/cycle (R²>0.5)` **그리고** 후반 50% 총증가 `> max(5%, 20MB)` 면 FAIL, 한쪽만이면 FLAG.
- **L3·L4**: GPU 카운트·mesh len·explorer view 수·surface·proc 수는 최종 체크포인트가 기준선과 **정수 일치**해야 PASS(1 이라도 순증가면 FAIL — 재현 불요 확정). 핸들 수만 자연 요동 허용치(64) 내 복귀.
- exit code 0=PASS / 1=FLAG / 2=FAIL. 플롯은 matplotlib 있을 때만.

주의: 기준선은 첫 post-warmup 체크포인트다. 짧은 런에서는 lazy 초기화(첫 markdown surface 의 텍스처 등)가 기준선 이후에 발생해 false FAIL 이 날 수 있다 — 본 실행은 충분히 길게, 판정 전 모든 surface kind 가 한 번씩 돌았는지 확인.

추가 주의 (macOS 사이클 실측 교훈, 2026-07-12):

- **s8 은 300s 로 측정 불능** — cycle=30s 라 기본 checkpoint(10 cycle)로는 마지막에 2초 간격
  체크포인트 2개만 남는다. idle 드리프트는 `SOAK_CHECKPOINT_EVERY=1` + 600s 이상으로 잰다.
- **proc_count 는 zombie 를 못 본다** — sysinfo 가 Z 상태 프로세스를 세지 않는다. L4 의 zombie
  축적은 `ps -axo stat=,ppid=` 외부 감시로 별도 계수해야 한다 (macOS 사이클에서 이 맹점 뒤로
  PTY 셸 zombie 누적이 숨어 있었다).
- **RSS 단독 판정 금지 (macOS)** — 메모리 압축기가 idle 페이지를 RSS 에서 숨긴다. phys
  footprint(`vmmap --summary`) 를 병행 측정해 이중 확인한다. 반대로 활동 직후 RSS 상승이
  footprint 평평과 동반되면 pool/캐시 워밍업이지 누수가 아니다.
- **`leaks` 는 diff 로만** — 절대값은 AppKit/XPC 부팅 노이즈를 포함한다 (Apple 자체 바이너리도
  ROOT LEAK 를 보고). warmup 직후와 종료 직전 2점을 떠 성장분만 판정한다.
- **알려진 상류 잔류 — egui `WidgetRects` layer 누적**: surface churn 시나리오(s1·s2·s6)의
  root RSS FLAG 에는 egui(0.31, master 도 동일)의 구조적 성장분이 포함된다.
  `WidgetRects::clear()` 가 `by_layer` 의 Vec 내용만 비우고 map 항목(capacity 포함)을 제거하지
  않아, surface 마다 고유 `Area` id 를 쓰는 tasty 에서 닫힌 surface 의 layer 항목(~10KB)이
  영구 잔존한다. 닫기 1회당 ~10–12KB — 공개 API 로 purge 불가(상류 몫). soak 판정 시 이
  성분(닫기 횟수 × ~10KB)을 알려진 잔류로 차감하고 그 이상의 성장만 신규 의심으로 본다.
  **상류 제보·자체 patch 는 하지 않기로 결정(2026-07-12)** — 규모가 작고 bounded-per-surface 라
  차감 운용으로 충분. egui 메이저 업그레이드 시 재확인만 한다 (우회안인 Area id 재사용은
  위젯 상태 bleed 리스크로 기각됨).

## 3단계 — 원인규명 (플랫폼별 런북)

FLAG/FAIL 이 재현되면(같은 시나리오 2회) 해당 시나리오만 도구 아래서 재실행한다. `children_rss_by_name` 이 plugin 프로세스를 지목하면 그 plugin 바이너리에 도구를 붙인다.

### Linux — heaptrack (L2 주력)

```bash
sudo apt install heaptrack heaptrack-gui   # 또는 배포판 패키지
# 하네스가 띄운 tasty 에 attach (soak meta 라인의 pid)
heaptrack -p <pid>
# 종료 후
heaptrack_gui heaptrack.tasty.<pid>.zst    # "Consumed over time" → 성장 콜스택
```

### macOS — Instruments / leaks (L1 최고 정밀 + L2)

```bash
# 라이브 reachability 판정 (soak 도중 반복 호출 가능 — 짧은 suspend 발생)
leaks <pid>
# 스택까지: 환경변수 후 재실행
MallocStackLogging=1 <soak 실행> ; leaks <pid>
# GUI 분석 (Allocations 구간 diff + Leaks)
xcrun xctrace record --template 'Leaks' --attach <pid> --output soak.trace
```

### Windows — UMDH (L2 스냅샷 diff)

Rust 기본 Windows 할당자는 HeapAlloc 경유라 UMDH 가 동작한다. Debugging Tools for Windows(WinSDK) 필요.

```powershell
gflags /i tasty.exe +ust          # 스택 캡처 켜기 (1회, 이후 spawn 부터 적용)
umdh -p:<pid> -f:snap1.log        # soak 초반 스냅샷
umdh -p:<pid> -f:snap2.log        # 충분히 성장한 뒤
umdh snap1.log snap2.log > diff.txt   # 증가 스택 상위부터 정렬됨
gflags /i tasty.exe -ust          # 끝나면 끄기
```

### 전 OS — dhat-heap (opt-in feature)

UMDH 미설치/무권한 환경에서 쓰는 크로스플랫폼 heap attribution. 전 할당을 계측해 **정상 종료 시** cwd 에 `dhat-heap.json` 을 남긴다.

```bash
cargo build --features dhat-heap
# 계측 인스턴스로 시나리오 재현 → system.shutdown(debug) 등으로 정상 종료
# dhat-heap.json 의 pps[] 를 분석 (뷰어: https://nnethercote.github.io/dh_view/dh_view.html)
```

해석 주의 2가지 (s6 조사 실측 교훈):

1. **shutdown 캐스케이드가 해제하는 L2 누수는 exit-time 잔류(`eb`)에 안 남는다** — heap 최대점 스냅샷(`gb`, t-gmax)으로 봐야 한다. 성장분이 부팅 피크를 넘도록 사이클을 충분히 돌려야 gmax 가 churn 말미에 찍힌다.
2. **비-Rust-heap 성장은 dhat 에 안 보인다** — mmap(플랫폼 shm, plugin shared buffer), GPU 드라이버 풀, GDI 등. "RSS 는 느는데 dhat gmax 총량이 평평"이면 이 유형 (s6 의 markdown 누수가 정확히 이랬다 — shared buffer 매핑 누적).

### L1 확정 도장 — ASAN+LSAN (Linux, nightly)

```bash
rustup +nightly target add x86_64-unknown-linux-gnu
RUSTFLAGS="-Zsanitizer=address" cargo +nightly test --release \
  --target x86_64-unknown-linux-gnu --test soak_memory -- --ignored
# 정상 종료 시 LSAN 리포트. GPU 드라이버 false positive 는
# LSAN_OPTIONS=suppressions=lsan.supp 로 억제 목록 관리.
```

miri 는 FFI(wgpu/ConPTY) 때문에 불가. valgrind 는 20~50배 감속이라 ASAN 이 커버하지 못하는 의문이 남을 때만(llvmpipe+xvfb 필요).

## GPU 카운트 수동 확인

soak 없이도 실행 중 인스턴스에 바로 물을 수 있다:

```bash
tasty list gpu-stats   # wgpu 리포트 + 창별 mesh 맵/atlas/draw calls/explorer view 수 (JSON)
```

surface 를 열고 닫은 전후로 두 번 찍어 `textures/buffers.allocated` 와 `egui_mesh_*_targets` 가 복귀하는지 보면 L3 를 즉석 판정할 수 있다.

## 관련

- [crash-diagnostics](crash-diagnostics.md) — panic/hang 진단 (누수가 아니라 죽음/멈춤일 때)
- [perf-benchmarks](perf-benchmarks.md) — 렌더 성능 측정 (RSS/GPU 메모리는 그쪽 범위 밖, 여기가 담당)
- [model-view-split](model-view-split.md) — `drop_view` 누락이 만드는 L2/L3 누수의 설계 차원 방지
- [e2e-tests](e2e-tests.md) — soak 이 재활용하는 `TastyInstance` 격리 하네스
- ADR-0034 — 셸/conhost 좀비(L4) 의 과거 사례와 Job Object 방어
