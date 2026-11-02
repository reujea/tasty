//! Busy tick에서 셸 통합 안내의 마감을 판정한다.
//! 출력 이벤트가 오지 않아도 마감이 지나면 안내가 나오도록 출력 경로와 분리했다.
//! parked engine은 배너를 보여 줄 창이 없으므로 판정하지 않고 창에 붙을 때까지 미룬다.

use crate::app::App;
use crate::app::command::CoreEvent;
use crate::app::dispatch_domain::DispatchSource;
use crate::app::window_access::engines_mut;

impl App {
    pub(crate) fn poll_shell_integration_hints(&mut self) {
        let mut pending = Vec::new();
        for (id, _, mut engine) in engines_mut!(self).window_entries() {
            for (surface_id, generation) in engine.take_due_shell_integration_hints() {
                pending.push((
                    DispatchSource::Engine(id),
                    CoreEvent::TerminalShellIntegrationHint {
                        generation,
                        surface_id,
                    },
                ));
            }
        }
        for (source, event) in pending {
            self.handle_core_event_system(source, event);
        }
    }
}
