//! 워크스페이스 close 구간 계측 (C1~C5, target: `tasty::close`) 의 공유 상태·헬퍼.
//!
//! 부팅(`src/boot/trace.rs`, `tasty::boot`) / 종료(`src/app/shutdown_trace.rs`,
//! `tasty::shutdown`) 계측과 같은 관례를 따른다: 상시 발화, 레벨 `info!`, 소요는
//! `ms` 필드(f64 밀리초). debug 빌드는 `$TASTY_HOME/debug-dev.log`(debug 레벨 file
//! layer)에 수집되고 stderr 기본 필터가 warn 이라 콘솔 노이즈는 없다. release
//! 검증은 `TASTY_LOG=info`.
//!
//! 두 모듈과 달리 이 모듈이 crate 루트에 있는 이유: close 경로는 `core`(도메인
//! cascade) / `state`(surface cleanup) / `app`(cascade 소비) 세 계층에 걸쳐 있고
//! headless 빌드에서도 `core` 쪽 경로가 살아 있어 `gui` gate 아래 둘 수 없다.
//!
//! **surface 단위가 아니라 합계로 찍는다** — 종료 계측 S5b(`PtyBackend::drop` 누적)
//! 선례와 같다. 탭 30개짜리 워크스페이스를 닫을 때 surface 마다 5줄씩 찍으면 로그가
//! 150줄로 폭증하고, 그 write 비용 자체가 close 구간에 들어가 측정을 왜곡한다.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 계측 로그의 `ms` 필드 값 — 부팅/종료 계측과 같은 표기(f64 밀리초).
pub(crate) fn elapsed_ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// `Duration` → `ms` 필드 값. 누적 합계(C5a~C5d)용.
pub(crate) fn duration_ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// cascade(IPC) 경로의 close t0.
///
/// GUI 경로(`AppState::close_workspace_at`)는 한 함수 안에서 끝나 지역 `Instant`
/// 로 충분하지만, cascade 경로는 `Core::close_case_workspace`(도메인 — 스냅샷 +
/// 대상 수집)와 `cascade_surface_closed`(cascade — 실제 cleanup)로 **함수가 갈린다**.
/// 두 구간을 하나의 `close_total` 로 묶으려면 함수 경계를 넘는 t0 이 필요하다.
///
/// 원샷이 아니라 **재무장(re-arm)** 이다 — 종료 t0(`SHUTDOWN_T0`)과 달리 close 는
/// 세션 중 몇 번이든 반복된다. `take` 로 소비되며, cascade 가 workspace level 이
/// 아니어서 소비되지 않은 값은 다음 `arm` 이 덮어쓴다.
static CASCADE_T0: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// cascade 계측 셀 락의 poison 복구 공용 보고 좌표(첫-1 회). 셀은 한 칸이라 복구는 안전하다.
static CASCADE_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const CASCADE_WHAT: &str = "close-trace cascade cell";

/// cascade workspace close 진입(`close_case_workspace` 선두)에서 호출.
/// `snapshot` 은 그 close 가 C1/C2 를 탔는지 — `close_total` 에 그대로 실린다.
pub(crate) fn arm_cascade(t0: Instant, snapshot: bool) {
    let mut g = crate::poison::recover_mutex(CASCADE_T0.lock(), CASCADE_WHAT, &CASCADE_POISONED);
    *g = Some((t0, snapshot));
}

/// cascade cleanup 완료 지점에서 호출 — 무장된 t0 을 소비한다. `None` 이면 이번
/// cascade 는 workspace level 이 아니었다는 뜻이라 `close_total` 을 찍지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn take_cascade() -> Option<(Instant, bool)> {
    let mut g = crate::poison::recover_mutex(CASCADE_T0.lock(), CASCADE_WHAT, &CASCADE_POISONED);
    g.take()
}

/// C1 — 스냅샷 캡처. `item` 규모(surface 수 / 인라인 스크롤백 라인 수)를 함께
/// 찍어야 ms 가 무엇에 비례하는지 판정할 수 있다. `persist_closed_scrollback`
/// (C2b) 이 라인을 디스크로 내리기 **전에** 불러야 라인 수가 0 이 아니다.
pub(crate) fn log_snapshot(t: Instant, item: &crate::model::ClosedItem, path: &'static str) {
    let extent = tasty_model::closed_item::snapshot_extent(item);
    tracing::info!(
        target: "tasty::close",
        ms = elapsed_ms(t),
        surfaces = extent.surfaces,
        lines = extent.scrollback_lines,
        path,
        "C1 snapshot (capture_workspace_snapshot)"
    );
}

/// C3 — pane x tab x leaf 3중 순회로 cleanup 대상 수집.
pub(crate) fn log_collect(t: Instant, surfaces: usize, path: &'static str) {
    tracing::info!(
        target: "tasty::close",
        ms = elapsed_ms(t),
        surfaces,
        path,
        "C3 collect_targets (pane x tab x leaf walk)"
    );
}

/// C4 — workspace scope memory purge (sqlite 풀스캔).
pub(crate) fn log_ws_purge(t: Instant, path: &'static str) {
    tracing::info!(
        target: "tasty::close",
        ms = elapsed_ms(t),
        path,
        "C4 ws_memory_purge (purge_scope(Workspace))"
    );
}

/// `close_total` — close 진입 → 완료. `snapshot` 은 C1/C2 를 실제로 탔는지다
/// (cascade/inline 경로는 `save_snapshot=false` 로 두 단계를 통째로 건너뛴다).
/// `close_total` 과 단계 합의 차이는 미계측 구간의 크기를 뜻한다.
pub(crate) fn log_total(t0: Instant, surfaces: usize, snapshot: bool, path: &'static str) {
    tracing::info!(
        target: "tasty::close",
        ms = elapsed_ms(t0),
        surfaces,
        snapshot,
        path,
        "close_total (workspace close)"
    );
}

/// `cleanup_surface` 세부(C5a~C5d)의 surface 간 누적기.
///
/// `cleanup_surface` 는 GUI(`cleanup_targets`)/cascade(`reclaim_closed_surfaces`)
/// 양쪽 루프에서 불리므로 누적은 호출자가 소유하고, `cleanup_surface` 는 여기에
/// 더하기만 한다.
#[derive(Default, Clone, Copy)]
pub(crate) struct CleanupSums {
    /// cleanup 한 surface 수. 0 이어도 로그는 발화한다 — "안 걸렸다" 와 "계측이
    /// 없다" 를 구분할 수 있어야 한다.
    pub(crate) surfaces: u64,
    /// C5a — `scrollback_store::delete` (`fs::remove_file`)
    pub(crate) scrollback_delete: Duration,
    /// C5b — `Terminal` drop (PTY kill). 필드 drop 까지 포함한 실제 소요.
    pub(crate) terminal_drop: Duration,
    /// C5c — host-side per-surface 인덱스 해제 (observer 워커 sender drop —
    /// join 은 S3b 로 미룬다, ADR-0076). observer 수에 비례하지 않는다.
    pub(crate) indices_drop: Duration,
    /// C5d — `purge_scope(Scope::Surface)` (sqlite 풀스캔). surface 당 **1회** —
    /// 과거엔 `SurfaceMetaStore::remove` 가 같은 purge 를 한 번 더 돌려 별도 단계
    /// (C5c meta_remove)로 잡혔다. 중복을 걷어내면서 그 단계도 함께 사라졌다.
    pub(crate) memory_purge: Duration,
}

impl CleanupSums {
    /// C5 + C5a~C5d 를 한 번에 발화한다. `total` 은 cleanup 루프 전체 소요(로그
    /// 자신을 뺀 값) — `total` 과 세부 합의 차이가 크면 미계측 구간(예:
    /// `enqueue_surface_closed`)이 남아 있다는 신호다.
    pub(crate) fn log(&self, total: Duration, path: &'static str) {
        tracing::info!(
            target: "tasty::close",
            ms = duration_ms(total),
            surfaces = self.surfaces,
            path,
            scrollback_delete_ms = duration_ms(self.scrollback_delete),
            terminal_drop_ms = duration_ms(self.terminal_drop),
            indices_drop_ms = duration_ms(self.indices_drop),
            memory_purge_ms = duration_ms(self.memory_purge),
            "C5 cleanup_targets (C5a scrollback_delete / C5b terminal_drop / C5c indices_drop / C5d memory_purge)"
        );
    }
}

/// `push_closed_item` 내부 세부(C2a~C2c) 소요. `CoreState::push_closed_item` 이
/// 돌려주고, workspace close 경로의 호출자만 이를 로그로 찍는다 — tab/pane close
/// 는 같은 함수를 타지만 계측 대상이 아니라 값을 그냥 버린다.
#[derive(Default, Clone, Copy)]
pub(crate) struct PushClosedItemTimings {
    /// C2a — `restore.command` surface meta 조회 (surface 마다 sqlite 1회)
    pub(crate) restore_inject: Duration,
    /// C2b — 캡처된 스크롤백 디스크 write (`~/.tasty/scrollback/<id>.bin`)
    pub(crate) scrollback_persist: Duration,
    /// C2c — LIFO 상한 초과 시 evict 된 항목의 스크롤백 파일 삭제
    pub(crate) evict: Duration,
}

impl PushClosedItemTimings {
    /// C2 를 발화한다. `total` 은 `push_closed_item` 호출 전체 소요.
    pub(crate) fn log(&self, total: Duration, path: &'static str) {
        tracing::info!(
            target: "tasty::close",
            ms = duration_ms(total),
            path,
            restore_inject_ms = duration_ms(self.restore_inject),
            scrollback_persist_ms = duration_ms(self.scrollback_persist),
            evict_ms = duration_ms(self.evict),
            "C2 push_closed_item (C2a restore_inject / C2b scrollback_persist / C2c evict)"
        );
    }
}
