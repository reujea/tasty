//! 자동 재시작(무응답·namespace 연속 만료)과 부팅 기동이 실패했을 때의 재시도.
//!
//! 두 기동은 사용자가 지켜보지 않는 사이 일어나므로, 그 기동이 한 번 실패했다고 멈추면
//! plugin 이 켜진 채 돌지 않는 상태로 남는다. 사용자가 켠 기동의 실패는 재시도하지 않고 바로
//! 알린다. 자동 재시작과 부팅이 시작한 기동은 연결에 성공할 때까지
//! 실패를 연속으로 세고, `RESTART_FAILURE_LIMIT` 번째 실패에서 자동 비활성화한다. 그 전에는
//! 실패마다 `RESTART_FAILURE_WINDOW × 2^(n-1)` 뒤 다시 띄운다. 별도 백오프 상수를 두지 않고
//! 실패 창을 기다림의 단위로 쓴다. 창을 넘겨 다시 띄우므로 10초 창의 연속 실패 집계와 겹쳐
//! 세지 않는다. 사용자가 켜거나 끄면 연속 기록을 지운다. 파일 교체(번들 업데이트)는 기다리던
//! 재시도와, 자신이 멈추거나 회수 뒤 기동을 가져오는 자동 재시작을 넘겨받아 쓰기 뒤 새 파일로
//! 띄운다.
//! 설치본이 바뀌지 않은 채 교체가 실패하면 넘겨받은 재시도를 되돌린다.

use std::time::Instant;

use super::{PluginManager, PluginTick, RESTART_FAILURE_LIMIT, RESTART_FAILURE_WINDOW};

/// 자동 재시작으로 시작한 기동의 연속 실패와 다음 재시도 시각.
#[derive(Debug, Default, Clone)]
pub(crate) struct RespawnStreak {
    pub(super) failures: usize,
    pub(super) due: Option<Instant>,
}

/// 파일 교체가 넘겨받은 자동 재시작 재시도. 설치본이 바뀌지 않은 실패에서 되돌릴 때 쓴다.
#[derive(Debug)]
#[must_use = "a taken retry is either carried out by the swap or restored; dropping it leaves an \
              enabled plugin down"]
pub struct TakenRetry {
    failures: usize,
    due: Instant,
}

/// 파일 교체를 시작한 결과. [`PluginManager::begin_file_swap`]이 돌려준다.
#[derive(Debug)]
pub struct FileSwap {
    /// 실행 중인 프로세스를 멈췄다. 교체 끝에 다시 띄운다.
    pub swapping: bool,
    /// 교체가 넘겨받은 기동 예약(회수 뒤 기동이나 자동 재시작 재시도)이 있다.
    pub restart: bool,
    /// 넘겨받은 자동 재시작 재시도. 설치본이 바뀌지 않은 실패에서
    /// [`PluginManager::restore_respawn_retry`]로 되돌린다.
    pub retry: Option<TakenRetry>,
}

impl PluginManager {
    /// 자동 재시작을 시작한다. 이 뒤의 기동 실패는 재시도 대상이다. 이미 진행 중인 연속 기록은 유지한다.
    pub(super) fn begin_respawn_streak(&mut self, plugin_id: &str) {
        self.respawn_streaks
            .entry(plugin_id.to_string())
            .or_default();
    }

    /// 연결 성공이나 사용자 조작으로 연속 기록을 지운다.
    pub(super) fn end_respawn_streak(&mut self, plugin_id: &str) {
        if self.respawn_streaks.remove(plugin_id).is_some() {
            self.sync_respawn_timer();
        }
    }

    /// 재시도를 기다리는 중인가. 다음 시도가 예약돼 있고 아직 띄우지 않은 상태다.
    pub(crate) fn awaits_respawn_retry(&self, plugin_id: &str) -> bool {
        self.respawn_streaks
            .get(plugin_id)
            .is_some_and(|s| s.due.is_some())
    }

    /// 파일 교체가 자동 재시작을 넘겨받는다. 넘겨받는 경우는 재시도를 기다리던 때와,
    /// 교체가 그 기동을 가져가는 때(`taking_start`: 프로세스를 멈추고 끝에 다시 띄우거나, 옛
    /// 프로세스의 회수 뒤 기동을 가져옴)다. 이때만 연속 기록과 예약을 지워 교체 중에 옛 파일로
    /// 띄우지 않게 한다. 그 밖(연결을 기다리는 프로세스를 그대로 두는 교체)에는 기록을 남겨 그
    /// 연결의 실패가 재시도로 이어지게 한다.
    ///
    /// 넘겨받으면 그 기록을 돌려준다. 연결을 기다리던 자동 재시작이면 예약 시각은 지금이다.
    /// 호출자가 쓰기 뒤 새 파일로 띄운다. 그 기동은 사용자 동작이므로 실패해도 자동 재시작의
    /// 연속 실패로 세지 않는다. 설치본이 바뀌지 않은 실패에서는 [`Self::restore_respawn_retry`]로
    /// 되돌린다.
    pub(crate) fn take_respawn_retry(
        &mut self,
        plugin_id: &str,
        taking_start: bool,
    ) -> Option<TakenRetry> {
        let streak = self.respawn_streaks.get(plugin_id)?;
        let due = match streak.due {
            Some(due) => due,
            None if taking_start => Instant::now(),
            None => return None,
        };
        let taken = TakenRetry {
            failures: streak.failures,
            due,
        };
        self.end_respawn_streak(plugin_id);
        Some(taken)
    }

    /// 설치본을 바꾸지 않고 끝난 교체가 넘겨받았던 재시도를 되돌린다. 실패 수와 예약 시각을
    /// 그대로 살려 원래의 백오프·한도가 이어서 처리한다. 예약 시각이 지났으면 다음 tick에서
    /// 띄운다. 옛 프로세스를 아직 회수하는 중이면 회수 뒤 기동으로 되돌려 파일·포트 사용이
    /// 겹치지 않게 하고, 그 기동은 연속 기록에 이어 센다. 그 사이 꺼졌거나 제거됐거나 이미 떠
    /// 있으면 되돌리지 않는다.
    pub fn restore_respawn_retry(&mut self, plugin_id: &str, retry: TakenRetry) {
        let wanted = !self.config.is_disabled(plugin_id)
            && !self.is_auto_disabled(plugin_id)
            && !self.processes.contains_key(plugin_id)
            && self.packages.iter().any(|p| p.manifest.id == plugin_id);
        if !wanted || self.respawn_streaks.contains_key(plugin_id) {
            return;
        }
        tracing::info!(
            plugin_id,
            failures = retry.failures,
            "plugin file swap left the install unchanged — restoring restart retry"
        );
        let due = if self.rearm_respawn_after_retire(plugin_id) {
            None
        } else {
            Some(retry.due)
        };
        self.respawn_streaks.insert(
            plugin_id.to_string(),
            RespawnStreak {
                failures: retry.failures,
                due,
            },
        );
        self.sync_respawn_timer();
    }

    /// 기동 실패를 자동 재시작의 연속 실패로 센다. 자동 재시작 중이 아니면 아무것도 하지 않는다.
    /// 한도에 닿아 자동 비활성화했으면 누적 실패 수를 돌려준다.
    pub(super) fn note_respawn_failure(
        &mut self,
        plugin_id: &str,
        already_disabled: bool,
        now: Instant,
    ) -> Option<usize> {
        let streak = self.respawn_streaks.get_mut(plugin_id)?;
        streak.failures += 1;
        let failures = streak.failures;
        let wanted = !already_disabled
            && !self.config.is_disabled(plugin_id)
            && self.packages.iter().any(|p| p.manifest.id == plugin_id);
        if !wanted {
            self.end_respawn_streak(plugin_id);
            return None;
        }
        if failures >= RESTART_FAILURE_LIMIT {
            tracing::error!(
                "plugin '{plugin_id}' failed {failures} automatic restarts in a row — \
                 auto-disabling until manual re-enable"
            );
            self.auto_disabled.insert(plugin_id.to_string());
            self.end_respawn_streak(plugin_id);
            return Some(failures);
        }
        let wait = RESTART_FAILURE_WINDOW * (1u32 << (failures - 1));
        streak.due = Some(now + wait);
        tracing::warn!(
            plugin_id,
            failures,
            wait_ms = wait.as_millis() as u64,
            "plugin restart failed — retrying"
        );
        self.sync_respawn_timer();
        None
    }

    /// [`PluginTick::Respawn`] 한 번 — 시각이 된 재시도를 띄운다.
    pub(super) fn poll_respawn_retries(&mut self, now: Instant) {
        let due: Vec<String> = self
            .respawn_streaks
            .iter_mut()
            .filter(|(_, s)| s.due.is_some_and(|d| d <= now))
            .map(|(id, s)| {
                s.due = None;
                id.clone()
            })
            .collect();
        for id in due {
            if self.is_auto_disabled(&id) {
                self.end_respawn_streak(&id);
                continue;
            }
            tracing::info!(plugin_id = id, "retrying plugin restart");
            self.start_if_still_wanted(&id);
            self.end_streak_without_start(&id);
        }
        self.sync_respawn_timer();
    }

    /// 꺼졌거나 제거돼 띄우지 않았으면 기다릴 결과가 없으므로 연속 기록을 지운다. 기동이 곧바로
    /// 실패해 다음 재시도가 예약됐으면 남긴다.
    pub(super) fn end_streak_without_start(&mut self, plugin_id: &str) {
        let waiting = self.processes.contains_key(plugin_id) || self.is_retiring(plugin_id);
        if !waiting
            && self
                .respawn_streaks
                .get(plugin_id)
                .is_some_and(|s| s.due.is_none())
        {
            self.end_respawn_streak(plugin_id);
        }
    }

    /// 가장 이른 재시도 시각에 타이머를 맞춘다. 남은 재시도가 없으면 내린다.
    pub(super) fn sync_respawn_timer(&mut self) {
        match self.respawn_streaks.values().filter_map(|s| s.due).min() {
            Some(at) => {
                self.timers
                    .once_at(PluginTick::Respawn, at, tasty_timer::Precision::Strict)
            }
            None => self.timers.cancel(PluginTick::Respawn),
        }
    }
}

/// 쓰는 시험이 Unix 전용(bash 가짜 plugin)이라 같은 조건으로 묶는다.
#[cfg(all(test, unix))]
impl PluginManager {
    /// 자동 재시작의 기동이 `failures`번 실패해 재시도를 기다리는 상태를 만든다.
    pub(crate) fn fail_automatic_restarts_for_test(&mut self, plugin_id: &str, failures: usize) {
        self.begin_respawn_streak(plugin_id);
        for _ in 0..failures {
            self.fail_start_for_test(plugin_id);
        }
    }

    /// 연결 실패 하나를 기록한다. 재시도는 실패 창 뒤라 10초 창 집계와 겹치지 않으므로 창 기록은 비운다.
    pub(crate) fn fail_start_for_test(&mut self, plugin_id: &str) {
        self.on_plugin_spawn_failure(
            plugin_id,
            super::FailureKind::ConnectFailed,
            anyhow::anyhow!("did not connect within 10s"),
        );
        self.spawn_failures.clear();
    }
}
