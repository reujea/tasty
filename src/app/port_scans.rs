//! App owns scan workers and result channels; Views retain only presentation and request identity.
use crate::adapters::ui::popup::port_scanner::{PortRowView, ScanSnapshot, SourceTag, format_addr};
use crate::view::ui::View;
use std::collections::HashSet;
use std::sync::{Weak, mpsc};
use std::thread::JoinHandle;
use winit::window::WindowId;

const MAX_SCAN_JOBS: usize = 8;

struct ScanJob {
    window: WindowId,
    view: Weak<()>,
    ticket: Weak<()>,
    favorites: bool,
    result: mpsc::Receiver<ScanResult>,
    worker: JoinHandle<()>,
}
#[derive(Default)]
pub(crate) struct PortScans {
    jobs: Vec<ScanJob>,
    stopping: bool,
}
impl PortScans {
    pub(crate) fn begin_shutdown(&mut self) {
        self.stopping = true;
    }

    /// Returns the count still owned, not an elapsed-time approximation of completion.
    pub(crate) fn poll_shutdown(&mut self) -> usize {
        self.stopping = true;
        let mut index = 0;
        while index < self.jobs.len() {
            if !self.jobs[index].worker.is_finished() {
                index += 1;
                continue;
            }
            let job = self.jobs.swap_remove(index);
            if job.worker.join().is_err() {
                tracing::warn!("port scan worker panicked during App retirement");
            }
        }
        self.jobs.len()
    }
}
impl Drop for PortScans {
    fn drop(&mut self) {
        let pending = self.poll_shutdown();
        if pending != 0 {
            // Forced App teardown may abandon observation. Dropping JoinHandle detaches these
            // workers; it neither cancels OS inspection nor establishes that they joined.
            tracing::warn!(
                pending,
                "App dropped while port scan workers remain unjoined"
            );
        }
    }
}

impl super::App {
    pub(crate) fn poll_port_scans(&mut self) {
        self.collect_port_scan_results();
        if self.port_scans.stopping {
            return;
        }
        self.start_pending_port_scans();
    }

    fn collect_port_scan_results(&mut self) {
        let mut index = 0;
        while index < self.port_scans.jobs.len() {
            if !self.port_scans.jobs[index].worker.is_finished() {
                index += 1;
                continue;
            }
            let result = match self.port_scans.jobs[index].result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => Err("scan worker disconnected".into()),
            };
            let job = self.port_scans.jobs.swap_remove(index);
            // is_finished above proves the thread ended; a result alone does not prove a join.
            if job.worker.join().is_err() {
                tracing::warn!("port scan worker panicked");
            }
            let Some(view) = self
                .view
                .views
                .get_mut(&job.window)
                .and_then(|view| view.as_main_mut())
            else {
                continue;
            };
            if !view.base.state.matches_identity(&job.view) {
                continue;
            }
            let slot = if job.favorites {
                &mut view.state.port_favorites_scan
            } else {
                &mut view.state.port_scan
            };
            if slot.accept(&job.ticket, result) {
                view.mark_dirty();
            }
        }
    }

    fn start_pending_port_scans(&mut self) {
        for (&window, view) in &mut self.view.views {
            let Some(view) = view.as_main_mut() else {
                continue;
            };
            let identity = view.base.state.identity();
            for favorites in [false, true] {
                // Refresh replaces the View's pending snapshot, never an in-flight worker.
                // Defer admission without dropping that latest request until capacity is free.
                if self.port_scans.jobs.len() >= MAX_SCAN_JOBS
                    || self.port_scans.jobs.iter().any(|job| {
                        job.window == window
                            && job.view.ptr_eq(&identity)
                            && job.favorites == favorites
                    })
                {
                    continue;
                }
                let slot = if favorites {
                    &mut view.state.port_favorites_scan
                } else {
                    &mut view.state.port_scan
                };
                let Some((ticket, snapshot)) = slot.take_request() else {
                    continue;
                };
                match spawn_scan_worker(&self.view.proxy, snapshot) {
                    Ok((result, worker)) => self.port_scans.jobs.push(ScanJob {
                        window,
                        view: identity.clone(),
                        ticket,
                        favorites,
                        result,
                        worker,
                    }),
                    Err(error) => {
                        tracing::warn!(%error, "port scan worker spawn failed");
                        slot.accept(&ticket, Err(error.to_string()));
                        view.mark_dirty();
                    }
                }
            }
        }
    }
}

type ScanResult = Result<Vec<PortRowView>, String>;

/// 대기 중인 요청을 꺼낸 뒤에만 부른다. 이 함수 밖에서 proxy를 미리 clone하지 않는다.
/// 이 함수는 about_to_wait와 redraw_main_window마다 도는 poll_port_scans 경로에 있다.
/// macOS winit은 EventLoopProxy clone마다 런루프 source를 추가하고 CFRunLoopWakeUp을 부르고,
/// Wayland는 clone한 sender를 drop할 때마다 ping한다. 요청이 없을 때 clone하면 메인 루프가 쉬지 않는다.
fn spawn_scan_worker(
    proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
    snapshot: ScanSnapshot,
) -> std::io::Result<(mpsc::Receiver<ScanResult>, JoinHandle<()>)> {
    let (sender, result) = mpsc::channel();
    let wake = worker_wake(proxy);
    let worker = std::thread::Builder::new()
        .name("port-scan".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(|| run_scan(snapshot))
                .unwrap_or_else(|_| Err("scan worker panicked".into()));
            if sender.send(result).is_err() {
                return;
            } // App has retired; no result consumer remains.
            if wake.send_event(crate::AppEvent::TimerTick).is_err() {
                tracing::debug!("port scan completion after App event loop closed");
            }
        })?;
    Ok((result, worker))
}

#[cfg(test)]
thread_local! {
    /// 이 스레드에서 worker용 proxy를 clone한 횟수. X11 시험 루프는 clone으로 깨지 않으므로 clone 자체를 센다.
    static WORKER_WAKE_CLONES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn worker_wake(
    proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
) -> winit::event_loop::EventLoopProxy<crate::AppEvent> {
    #[cfg(test)]
    WORKER_WAKE_CLONES.with(|count| count.set(count.get() + 1));
    proxy.clone()
}

#[cfg(all(test, target_os = "linux"))]
#[path = "port_scans_tests.rs"]
mod tests;

/// Tasty 하위 프로세스의 소속을 모은 뒤 선택한 범위의 TCP 소켓을 조회한다.
fn run_scan(snapshot: ScanSnapshot) -> Result<Vec<PortRowView>, String> {
    let mut pid_to_source: std::collections::HashMap<u32, (String, Option<String>)> =
        std::collections::HashMap::new();
    for (_sid, shell_pid, path) in &snapshot.surfaces {
        let descendants = tasty_portscan::collect_descendant_pids(*shell_pid);
        for pid in descendants {
            pid_to_source
                .entry(pid)
                .or_insert_with(|| (path.workspace_name.clone(), path.tab_name.clone()));
        }
    }

    if snapshot.show_all_system {
        let all = tasty_portscan::scan_all();
        let rows: Vec<PortRowView> = all
            .into_iter()
            .map(|p| {
                let source = p
                    .pid
                    .and_then(|pid| pid_to_source.get(&pid))
                    .map(|(ws, tab)| SourceTag::Tasty {
                        workspace_name: ws.clone(),
                        tab_name: tab.clone(),
                    })
                    .unwrap_or(SourceTag::External);
                PortRowView {
                    port: p.port,
                    addr_display: format_addr(p.addr),
                    pid: p.pid,
                    process_name: p.process_name,
                    source,
                    state: p.state,
                    favorited: false,
                }
            })
            .collect();
        Ok(rows)
    } else {
        let tasty_pids: HashSet<u32> = pid_to_source.keys().copied().collect();
        let ports = tasty_portscan::scan_for_pids(&tasty_pids);
        let rows: Vec<PortRowView> = ports
            .into_iter()
            .map(|p| {
                let source = pid_to_source
                    .get(&p.pid)
                    .map(|(ws, tab)| SourceTag::Tasty {
                        workspace_name: ws.clone(),
                        tab_name: tab.clone(),
                    })
                    .unwrap_or(SourceTag::External);
                PortRowView {
                    port: p.port,
                    addr_display: format_addr(p.addr),
                    pid: Some(p.pid),
                    process_name: p.process_name,
                    source,
                    state: p.state,
                    favorited: false,
                }
            })
            .collect();
        Ok(rows)
    }
}
