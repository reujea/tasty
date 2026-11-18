//! Lua 워커의 HostCommand를 about_to_wait에서 메인 스레드 상태에 적용한다.
//!
//! `tasty.tree()`가 읽는 스냅샷은 job이 있을 때만 만든다. job을 보내기 직전에 그 시점의 트리를
//! 발행하고([`lua_send!`]), 보낸 job이 큐에 있거나 실행 중인 동안에는 iteration마다 다시 발행한다.
//! job이 없는 동안에는 스냅샷을 읽는 Lua 코드가 없다. VM을 닫을 때의 `__gc` finalizer도
//! 트리를 읽지 못한다(tasty-lua 시험이 고정한다).

use crate::adapters::ipc::handler::build_engine_tree;
use crate::app::App;
use crate::app::window_access::{EngineScan, engines_mut};
use crate::view::ui::View;
use tasty_lua::{HostCommand, LuaSnapshot};
use winit::window::WindowId;

/// App의 Lua 엔진으로 job을 보내는 [`LuaSend`](crate::hooks::lua::LuaSend). View·engine 필드만
/// 빌리므로 plugin manager·자동실행 상태를 함께 빌린 채로 쓸 수 있다. 엔진이 없으면 `None`이다.
macro_rules! lua_send {
    ($app:expr) => {
        $app.lua_engine.as_ref().map(|engine| {
            $crate::hooks::lua::LuaSend::new(engine, || {
                $crate::app::dispatch::lua_commands::lua_tree(
                    $crate::app::window_access::EngineScan::from_fields(
                        &$app.view.views,
                        &$app.engines,
                    ),
                )
            })
        })
    };
}
pub(crate) use lua_send;

/// 모든 MainView·parked workspace의 읽기 스냅샷. 세션 수·트리 크기에 비례해 비용이 든다.
pub(crate) fn lua_tree(scan: EngineScan<'_>) -> LuaSnapshot {
    let mut tree = Vec::new();
    for (s, e) in scan.sessions() {
        tree.extend(build_engine_tree(
            &s.navigation,
            s.active_workspace_index(e.core),
            &e.read(),
        ));
    }
    LuaSnapshot { tree }
}

/// 보낸 job이 큐에 있거나 실행 중일 때만 트리를 다시 발행한다. 실행 중인 job이 이번 iteration의
/// 상태 변화를 다음 `tasty.tree()`에서 보게 한다.
fn refresh_for_pending_jobs(
    engine: Option<&tasty_lua::LuaEngine>,
    tree: impl FnOnce() -> LuaSnapshot,
) {
    if let Some(engine) = engine
        && engine.pending_jobs() > 0
    {
        engine.publish_snapshot(tree());
    }
}

impl App {
    pub(crate) fn dispatch_pending_lua_commands(&mut self) {
        let Some(engine) = self.lua_engine.as_ref() else {
            return;
        };
        for cmd in engine.drain_commands() {
            match cmd {
                HostCommand::RunCli(args) => tasty_lua::run_tasty_cli(&args),
            }
        }
    }

    /// about_to_wait가 job 송신 단계 뒤에 부른다. 진행 중 job이 없으면 트리를 만들지 않는다.
    pub(crate) fn refresh_lua_snapshot_for_pending_jobs(&self) {
        refresh_for_pending_jobs(self.lua_engine.as_ref(), || lua_tree(self.engines()));
    }

    /// 스크립트 변경을 승인하면 해시 저장을 시도하고 이미 읽은 소스를 실행한다.
    /// 저장 실패는 경고만 남기며 실행을 막지 않는다.
    pub(crate) fn dispatch_pending_script_confirm(&mut self) {
        for (window, mut wait, source, name) in std::mem::take(&mut self.scripts_after_save) {
            match wait.poll() {
                crate::app::settings_change::SettingsPoll::Pending => {
                    self.scripts_after_save.push((window, wait, source, name));
                }
                _ => self.submit_shortcut_script(window, &source, name),
            }
        }
        let ids: Vec<WindowId> = self
            .view
            .views
            .iter()
            .filter_map(|(id, w)| {
                let main = w.as_main()?;
                let data = main.state.dialogs.approved_script.as_ref()?;
                data.result.map(|_| *id)
            })
            .collect();
        for id in ids {
            let Some(pending) = self
                .view
                .views
                .get_mut(&id)
                .and_then(|w| w.as_main_mut())
                .and_then(|m| m.state.dialogs.approved_script.take())
            else {
                continue;
            };
            if pending.result != Some(true) {
                continue; // 취소 — 폐기(이미 take 됨).
            }
            // 설정은 engine 마다 같은 사본을 둔다. 승인한 윈도우의 사본만 바꾸면 다른 윈도우가 자기 사본을
            // 저장할 때 이 해시를 지우므로 모든 사본에 넣은 뒤 승인한 윈도우의 사본을 저장한다.
            let approver = self.engines.of_window(id);
            let (mut mine, mut others) = (None, Vec::new());
            for session in self.engines.all_sessions_mut() {
                if Some(session.id) == approver {
                    mine = Some(&mut session.runtime.settings);
                } else {
                    others.push(&mut session.runtime.settings);
                }
            }
            let snapshot = record_script_hash(mine, others, &pending.script_id, &pending.new_hash);
            let (done, wait) = crate::app::settings_change::pair(self.services.reply_wake());
            if let Some(snapshot) = snapshot {
                self.services.submit_settings_save(
                    snapshot,
                    Some(crate::intent::IntentOrigin::System),
                    Some(done),
                );
                self.scripts_after_save.push((
                    id,
                    wait,
                    pending.source.clone(),
                    pending.name.clone(),
                ));
            } else {
                self.submit_shortcut_script(id, &pending.source, pending.name.clone());
            }
            if let Some((main, _)) = engines_mut!(self).window_pair(id) {
                main.mark_dirty();
            }
            // Keep the approved source in the continuation; never re-read it after saving.
        }
    }

    /// 사용자가 시작한 스크립트 실행을 요청한다. 실행하지 못하면 요청한 창에 실패 토스트를 띄운다.
    pub(crate) fn submit_shortcut_script(&mut self, window: WindowId, source: &str, name: String) {
        use crate::hooks::lua_runs::{RunOrigin, ShortcutFailure};
        let submitted = match lua_send!(self) {
            Some(lua) => lua.engine().submit_script(source, Some(&name), None),
            None => Err(tasty_lua::RunRejected::WorkerGone),
        };
        match submitted {
            Ok(ticket) => self
                .lua_runs
                .record(ticket, RunOrigin::Shortcut { window, name }),
            Err(rejected) => {
                tracing::warn!(target: "tasty_lua", "script '{name}' not queued: {rejected:?}");
                self.toast_script_failure(window, &name, &ShortcutFailure::Rejected(rejected));
            }
        }
    }

    /// 워커가 돌려준 실행 결과를 시작 경로로 보낸다. 자동실행 실패는 알림 채널, 단축키 실패는 원 창의 토스트다.
    pub(crate) fn settle_lua_runs(&mut self) {
        let Some(engine) = self.lua_engine.as_ref() else {
            return;
        };
        let outcomes = engine.drain_run_outcomes();
        if outcomes.is_empty() {
            return;
        }
        let settled = self.lua_runs.settle(outcomes);
        for notice in settled.notices {
            crate::system_notice::post(notice);
        }
        for (origin, failure) in settled.for_views {
            if let crate::hooks::lua_runs::RunOrigin::Shortcut { window, name } = origin {
                self.toast_script_failure(
                    window,
                    &name,
                    &crate::hooks::lua_runs::ShortcutFailure::Run(&failure),
                );
            }
        }
    }

    /// 원 창이 닫혔으면 다른 창에 옮기지 않고 버린다.
    fn toast_script_failure(
        &mut self,
        window: WindowId,
        name: &str,
        failure: &crate::hooks::lua_runs::ShortcutFailure<'_>,
    ) {
        let Some(main) = self
            .view
            .views
            .get_mut(&window)
            .and_then(|w| w.as_main_mut())
        else {
            tracing::debug!(target: "tasty_lua", "script '{name}' result arrived after its window closed");
            return;
        };
        main.state.toasts.push(
            crate::hooks::lua_runs::shortcut_failure_text(name, failure),
            crate::adapters::ui::ToastKind::Error,
            crate::adapters::ui::ToastScope::Window,
        );
        main.mark_dirty();
    }
}

/// 승인한 해시를 모든 설정 사본에 넣고 승인한 윈도우의 사본(`approver`)을 저장한다.
/// 승인한 윈도우의 engine 을 찾지 못하면 저장하지 않는다.
fn record_script_hash<'a>(
    approver: Option<&mut crate::settings::Settings>,
    others: impl IntoIterator<Item = &'a mut crate::settings::Settings>,
    script_id: &str,
    hash: &str,
) -> Option<crate::settings::Settings> {
    let observation = approver.as_ref().and_then(|s| s.file_observation.clone());
    for settings in others {
        settings.scripts.update_hash(script_id, hash.to_string());
        settings.file_observation = observation.clone();
    }
    match approver {
        Some(settings) => {
            settings.scripts.update_hash(script_id, hash.to_string());
            Some(settings.clone())
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;
    use std::cell::Cell;
    use std::time::{Duration, Instant};
    use tasty_lua::{HostCommand, LuaEngine, LuaSnapshot};

    fn named(name: &str) -> LuaSnapshot {
        LuaSnapshot {
            tree: vec![serde_json::json!({ "name": name })],
        }
    }

    fn wait_until(what: &str, done: impl Fn() -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while !done() {
            assert!(Instant::now() < end, "시간 안에 {what}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Given 진행 중 job이 없다, When iteration 갱신을 여러 번 한다, Then 트리를 만들지 않는다.
    /// Given 실행 중인 job, When 상태가 바뀐 뒤 갱신한다, Then 그 job의 다음 tree()가 새 값을 읽는다.
    /// Given job이 끝났다, When 다시 갱신한다, Then 더 발행하지 않는다.
    #[test]
    fn the_iteration_refresh_publishes_only_while_a_job_is_pending() {
        let engine = LuaEngine::new().expect("init");
        let built = Cell::new(0);
        let refresh = |name: &str| {
            super::refresh_for_pending_jobs(Some(&engine), || {
                built.set(built.get() + 1);
                named(name)
            })
        };
        for _ in 0..8 {
            refresh("idle");
        }
        assert_eq!(built.get(), 0, "job이 없으면 트리를 만들지 않는다");
        assert_eq!(engine.published_snapshots(), 0);

        engine.run_script(
            "while (tasty.tree()[1] or {}).name ~= 'changed' do end              tasty.run_cli({tasty.tree()[1].name})",
            None,
        );
        refresh("first");
        assert_eq!(
            engine.pending_jobs(),
            1,
            "이름이 바뀌기 전에는 끝나지 않는다"
        );
        refresh("changed");
        wait_until("job이 끝난다", || engine.pending_jobs() == 0);
        let cmds = engine.drain_commands();
        assert!(
            matches!(cmds.as_slice(), [HostCommand::RunCli(a)] if a == &["changed"]),
            "{cmds:?}"
        );
        let published = engine.published_snapshots();
        refresh("after");
        assert_eq!(
            engine.published_snapshots(),
            published,
            "끝난 뒤에는 발행하지 않는다"
        );
        super::refresh_for_pending_jobs(None, || unreachable!("엔진 없음"));
    }

    /// 두 스크립트를 해시 없이 등록한 사본과 그 id.
    fn with_scripts() -> (Settings, [String; 2]) {
        let mut settings = Settings::default();
        let ids = ["one", "two"].map(|name| {
            settings
                .scripts
                .add(name.into(), format!("/{name}.lua").into(), String::new())
        });
        (settings, ids)
    }

    /// 윈도우 둘이 각자 다른 스크립트를 승인해도 두 해시가 모두 저장된다.
    #[test]
    fn script_hashes_approved_in_two_windows_both_survive_a_restart() {
        let _home = crate::test_support::IsolatedHome::new();
        let (mut first, [one, two]) = with_scripts();
        let (mut second, _) = with_scripts();
        super::record_script_hash(Some(&mut first), [&mut second], &one, "hash-one")
            .expect("snapshot")
            .save()
            .expect("save");
        super::record_script_hash(Some(&mut second), [&mut first], &two, "hash-two")
            .expect("snapshot")
            .save()
            .expect("save");

        let restarted = Settings::load();
        let hash = |id: &str| restarted.scripts.get(id).map(|entry| entry.sha256.clone());
        assert_eq!(hash(&one).as_deref(), Some("hash-one"));
        assert_eq!(hash(&two).as_deref(), Some("hash-two"));
        assert_eq!(
            first.scripts.get(&two).map(|e| e.sha256.as_str()),
            Some("hash-two")
        );
    }

    /// 사본이 갈라졌으면 승인한 윈도우의 사본을 저장한다.
    #[test]
    fn the_approving_windows_copy_is_saved() {
        let _home = crate::test_support::IsolatedHome::new();
        let (mut approver, [one, _]) = with_scripts();
        let (mut other, _) = with_scripts();
        let only_here = approver
            .scripts
            .add("three".into(), "/three.lua".into(), String::new());
        super::record_script_hash(Some(&mut approver), [&mut other], &one, "hash-one")
            .expect("snapshot")
            .save()
            .expect("save");
        let restarted = Settings::load();
        assert!(restarted.scripts.get(&only_here).is_some());
        assert_eq!(
            restarted.scripts.get(&one).map(|e| e.sha256.as_str()),
            Some("hash-one")
        );
    }
}
