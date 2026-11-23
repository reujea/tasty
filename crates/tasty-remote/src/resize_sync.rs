//! mirror 크기 요청의 응답 대기와 재시도. 시각은 호출자가 넘기며 App·View를 참조하지 않는다.
//!
//! 서버가 `ipc.stream.resize-ack`를 알린 연결에서만 응답을 기다린다. 응답은 기다리는 요청과
//! 크기가 같은 Resize 또는 ResizeRejected다. surface마다 마지막 요청 하나만 기다리므로 늦게 온
//! 이전 요청의 응답은 최신 대기를 풀지 않는다.
//! 규칙: docs/dev-guide/attach-behavior.md#크기-요청의-응답과-재시도.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

/// 요청을 보낸 뒤 응답을 기다리는 시간. 지나면 실패한 시도로 본다.
pub const RESIZE_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// 기다리는 요청이 몇 번째 시도인지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    /// 크기 변경으로 처음 보낸 요청. 실패하면 한 번 자동으로 다시 보낸다.
    First,
    /// 자동으로 다시 보낸 요청. 실패하면 배너에 올린다.
    AutoRetry,
    /// 사용자가 배너에서 다시 시도한 요청. 자동 재시도 없이 바로 판정한다.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pending {
    started: Instant,
    target: (usize, usize),
    deadline: Instant,
    attempt: Attempt,
    /// 첫 시도가 거절됐다. 마감까지 기다렸다가 자동 재시도한다.
    rejected: bool,
}

/// 마감이 지나 다시 보내야 하는 요청. 호출자가 전송한 뒤 [`ResizeSync::note_resent`]로 알린다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resend {
    pub started: Instant,
    pub surface_id: u32,
    pub cols: usize,
    pub rows: usize,
}

/// 원격 surface별 크기 요청 상태. 키는 원격 surface ID다.
#[derive(Debug, Default)]
pub struct ResizeSync {
    /// 마지막으로 큐에 넣은 크기. 응답 전 같은 요청의 반복 전송을 막는다.
    /// 큐 전송 성공은 원격 적용 확인이 아니다.
    last_forwarded: HashMap<u32, (usize, usize)>,
    /// 이 연결의 서버가 모든 요청에 응답하는지. 구 서버면 기다리지 않는다.
    acks: bool,
    pending: HashMap<u32, Pending>,
    /// 자동 재시도 뒤에도 실패한 surface와 그 목표 크기. 배너가 이 목록을 보여 준다.
    failed: BTreeMap<u32, (usize, usize)>,
}

impl ResizeSync {
    /// Observation only; retry/failure policy remains owned by this state machine.
    pub fn pending_times(&self) -> impl Iterator<Item = Instant> + '_ {
        self.pending.values().map(|pending| pending.started)
    }

    /// 새 연결의 빈 상태. `acks`는 서버가 모든 요청에 응답하는지다.
    pub fn for_connection(acks: bool) -> Self {
        Self {
            acks,
            ..Self::default()
        }
    }

    /// 새 연결에 맞춰 모든 상태를 비운다. 옛 연결의 응답은 다시 오지 않는다.
    pub fn reset_for_connection(&mut self, acks: bool) {
        *self = Self::for_connection(acks);
    }

    /// 이 연결에서 응답을 기다리는지.
    pub fn acks_expected(&self) -> bool {
        self.acks
    }

    /// 같은 크기를 이미 보냈으면 false다.
    pub fn should_send(&self, surface_id: u32, cols: usize, rows: usize) -> bool {
        self.last_forwarded.get(&surface_id) != Some(&(cols, rows))
    }

    /// 크기 변경으로 요청을 큐에 넣었다. 같은 surface의 이전 대기는 이 요청으로 바뀐다.
    pub fn note_sent(&mut self, surface_id: u32, cols: usize, rows: usize, now: Instant) {
        self.last_forwarded.insert(surface_id, (cols, rows));
        self.wait(surface_id, (cols, rows), Attempt::First, now);
    }

    /// 마감으로 다시 보낸 요청을 큐에 넣었다.
    pub fn note_resent(&mut self, resend: Resend, now: Instant) {
        let target = (resend.cols, resend.rows);
        self.last_forwarded.insert(resend.surface_id, target);
        self.wait(resend.surface_id, target, Attempt::AutoRetry, now);
        if let Some(pending) = self.pending.get_mut(&resend.surface_id) {
            pending.started = resend.started;
        }
    }

    fn wait(&mut self, surface_id: u32, target: (usize, usize), attempt: Attempt, now: Instant) {
        if !self.acks {
            return;
        }
        self.pending.insert(
            surface_id,
            Pending {
                started: now,
                target,
                deadline: now + RESIZE_ACK_TIMEOUT,
                attempt,
                rejected: false,
            },
        );
    }

    /// 서버의 Resize. 기다리던 크기와 같을 때만 대기를 끝낸다. 다른 크기는 늦게 온 이전 응답이다.
    /// 기다리는 요청 없이 실패한 크기가 늦게 확정되면 실패 목록에서도 뺀다. 서버가 멈췄다가 밀린
    /// 요청을 처리한 경우다.
    pub fn on_resize(&mut self, surface_id: u32, cols: usize, rows: usize) {
        let answered = match self.pending.get(&surface_id) {
            Some(p) => p.target == (cols, rows),
            None => self.failed.get(&surface_id) == Some(&(cols, rows)),
        };
        if answered {
            self.pending.remove(&surface_id);
            self.failed.remove(&surface_id);
        }
    }

    /// 서버의 ResizeRejected. 첫 시도는 마감에 다시 보내고, 재시도의 거절은 바로 실패로 올린다.
    pub fn on_rejected(&mut self, surface_id: u32, cols: usize, rows: usize) {
        let Some(p) = self.pending.get_mut(&surface_id) else {
            return;
        };
        if p.target != (cols, rows) {
            return;
        }
        if p.attempt == Attempt::First {
            p.rejected = true;
        } else {
            let target = p.target;
            self.pending.remove(&surface_id);
            self.failed.insert(surface_id, target);
        }
    }

    /// 마감이 지난 요청을 처리한다. 첫 시도는 다시 보낼 목록으로, 재시도는 실패 목록으로 옮긴다.
    /// 다시 보낼 요청은 호출자가 전송한 뒤 [`ResizeSync::note_resent`]로 새 마감을 건다.
    pub fn take_due(&mut self, now: Instant) -> Vec<Resend> {
        let due: Vec<(u32, Pending)> = self
            .pending
            .iter()
            .filter(|(_, p)| p.deadline <= now)
            .map(|(&id, &p)| (id, p))
            .collect();
        let mut resend = Vec::new();
        for (surface_id, p) in due {
            self.pending.remove(&surface_id);
            if p.attempt == Attempt::First {
                // 같은 요청을 다시 보내야 하므로 중복 전송 방지 기록을 지운다.
                self.last_forwarded.remove(&surface_id);
                resend.push(Resend {
                    started: p.started,
                    surface_id,
                    cols: p.target.0,
                    rows: p.target.1,
                });
            } else {
                self.failed.insert(surface_id, p.target);
            }
        }
        resend.sort_by_key(|r| r.surface_id);
        resend
    }

    /// 마감이 지난 대기 중 `hidden`이 참인 surface의 것을 재전송·실패 이동 없이 버린다.
    /// 숨은 mirror는 보이게 될 때 새 요청을 받으므로 이 대기는 더 이상 쓸모가 없다.
    /// 마감 전 대기와 이미 확정된 실패는 그대로 둔다. [`ResizeSync::take_due`]보다 먼저 부른다.
    pub fn abandon_due(&mut self, now: Instant, mut hidden: impl FnMut(u32) -> bool) {
        let due: Vec<u32> = self
            .pending
            .iter()
            .filter(|(id, p)| p.deadline <= now && hidden(**id))
            .map(|(&id, _)| id)
            .collect();
        for id in due {
            self.abandon_pending(id);
        }
    }

    /// 이 surface의 대기를 결과 없이 버리고 중복 전송 기록도 지운다. 확정된 실패는 그대로 둔다.
    pub fn abandon_pending(&mut self, surface_id: u32) {
        self.pending.remove(&surface_id);
        self.last_forwarded.remove(&surface_id);
    }

    /// 배너의 다시 시도. 실패한 surface의 요청을 다시 보낼 목록으로 돌려주고 대기를 건다.
    /// 호출자는 반환된 요청을 모두 전송한다. 응답이 없거나 거절되면 다시 실패 목록에 오른다.
    pub fn retry_failed(&mut self, now: Instant) -> Vec<Resend> {
        let failed = std::mem::take(&mut self.failed);
        failed
            .into_iter()
            .map(|(surface_id, (cols, rows))| {
                self.last_forwarded.insert(surface_id, (cols, rows));
                self.wait(surface_id, (cols, rows), Attempt::Manual, now);
                Resend {
                    started: now,
                    surface_id,
                    cols,
                    rows,
                }
            })
            .collect()
    }

    /// 배너 닫기. 진행 중인 다시 시도는 계속 기다리되 배너에는 보이지 않는다.
    /// 이후 새로 실패하면 다시 목록에 오른다.
    pub fn dismiss_failed(&mut self) {
        self.failed.clear();
        for p in self.pending.values_mut() {
            if p.attempt == Attempt::Manual {
                // 자동 재시도와 판정이 같고 배너의 다시 시도 표시에서만 빠진다.
                p.attempt = Attempt::AutoRetry;
            }
        }
    }

    /// 배너에 보일 실패 surface. 원격 ID 순이다.
    pub fn failed(&self) -> impl Iterator<Item = u32> + '_ {
        self.failed.keys().copied()
    }

    /// 배너에 이름이 보일 surface. 실패 목록과 다시 시도 중인 surface이며 원격 ID 순이다.
    pub fn banner_surfaces(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .failed
            .keys()
            .copied()
            .chain(
                self.pending
                    .iter()
                    .filter(|(_, p)| p.attempt == Attempt::Manual)
                    .map(|(&id, _)| id),
            )
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// 배너의 다시 시도 응답을 기다리는 중인지.
    pub fn retrying(&self) -> bool {
        self.pending.values().any(|p| p.attempt == Attempt::Manual)
    }

    /// 가장 이른 마감. 타이머가 이 시각에 루프를 깨운다.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|p| p.deadline).min()
    }

    /// 손실 통지로 응답이 빠졌을 수 있다. 대기를 끝내고 중복 전송 기록을 지워 재동기화 뒤 다시 보내게 한다.
    /// 실패 목록은 그대로 둔다.
    pub fn forget_pending(&mut self) {
        self.pending.clear();
        self.last_forwarded.clear();
    }

    /// terminal mirror로 남은 surface만 유지한다. 닫히거나 다른 kind로 바뀐 surface의 상태를 지운다.
    pub fn retain_surfaces(&mut self, mut keep: impl FnMut(u32) -> bool) {
        self.last_forwarded.retain(|id, _| keep(*id));
        self.pending.retain(|id, _| keep(*id));
        self.failed.retain(|id, _| keep(*id));
    }
}

#[cfg(test)]
#[path = "resize_sync_tests.rs"]
mod tests;
