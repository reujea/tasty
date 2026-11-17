//! Native fake processes plus actual App APIs for GUI-only file consumers.
use super::*;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tasty_host_plugin::{known_plugins::KnownPlugins, manager::file_config::ConfigChange};
const ID: &str = "com.example.native-files";
const FAKE: &str = r#"#!/usr/bin/python3
import os,socket,json,pathlib
home=pathlib.Path(os.environ['TASTY_PARENT_HOME'])
with (home/'owned-plugin-pids').open('a') as log: log.write(str(os.getpid())+'\n')
s=socket.create_connection(('127.0.0.1',int(os.environ['TASTY_HOST_IPC_PORT'])))
f=s.makefile('rwb',buffering=0)
def send(value):f.write((json.dumps(value)+'\n').encode())
send({'plugin_id':os.environ['TASTY_PLUGIN_ID'],'token':os.environ['TASTY_PLUGIN_TOKEN']})
assert json.loads(f.readline())['auth_ack']['ok']
send({'event':{'kind':'hello','plugin_id':os.environ['TASTY_PLUGIN_ID'],'version':'1.0.0'}})
for line in f:
 request=json.loads(line)
 send({'id':request['id'],'result':{}})
 if request.get('method')=='shutdown':break
"#;
fn manifest(dir: &Path) {
    std::fs::write(
        dir.join("tasty-plugin.toml"),
        format!(
            r#"manifest_version=1
id="{ID}"
name="Owned native file consumer"
version="1.0.0"
api_version="1"
[entry]
type="process"
command="fake.py"
[[contributes.commands]]
id="probe"
title_i18n_key="command.probe"
scope="global"
default_keybinding="ctrl+shift+g"
"#
        ),
    )
    .unwrap();
}
fn pids(home: &Path) -> Vec<u32> {
    std::fs::read_to_string(home.join("owned-plugin-pids"))
        .unwrap_or_default()
        .lines()
        .map(|s| s.parse().unwrap())
        .collect()
}
fn tick(app: &mut App) {
    app.collect_plugin_file_shutdown();
    if let Some(mgr) = app.plugin_manager.as_mut() {
        drop(mgr.pump(Instant::now()));
    }
}
fn wait(app: &mut App, reply: &Reply) -> Value {
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        tick(app);
        if let Some(result) = reply.poll() {
            return result.unwrap_or_else(|error| panic!("native App callback: {error:?}"));
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn disable(app: &mut App, home: &Path) {
    let done = Reply::default();
    let completion = done.clone();
    app.enqueue_plugin_config(
        ConfigChange::Enabled {
            id: ID.into(),
            enabled: false,
        },
        move |_, result| completion.finish(result),
    )
    .unwrap();
    wait(app, &done);
    let end = Instant::now() + Duration::from_secs(5);
    while !app
        .plugin_manager
        .as_mut()
        .unwrap()
        .poll_file_retirement(ID)
    {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    for pid in pids(home) {
        assert!(
            !PathBuf::from(format!("/proc/{pid}")).exists(),
            "owned plugin PID not reaped: {pid}"
        );
    }
}
fn register(app: &mut App, home: &Path, dir: &Path) {
    let known = home.join("known-plugins.toml");
    let held = tasty_utils::file_replace::lock(&known).unwrap();
    let done = Reply::default();
    let completion = done.clone();
    app.enqueue_plugin_install(
        dir.into(),
        crate::app::plugin_glue::install_trust::InstallChoice::AllowUnverified,
        None,
        false,
        move |_, result| completion.finish(result),
    )
    .unwrap();
    tick(app);
    std::thread::sleep(Duration::from_millis(20));
    tick(app);
    assert!(done.poll().is_none());
    assert!(app.plugin_manager.as_ref().unwrap().packages().is_empty());
    assert!(pids(home).is_empty());
    drop(held);
    assert_eq!(wait(app, &done)["installed"], ID);
    assert!(pids(home).is_empty());
    assert_eq!(app.plugin_manager.as_ref().unwrap().packages()[0].dir, dir);
    assert!(
        KnownPlugins::load_from(known)
            .unwrap()
            .unverified_choice(ID)
            .is_some()
    );
    let done = Reply::default();
    let completion = done.clone();
    app.enqueue_plugin_config(
        ConfigChange::Enabled {
            id: ID.into(),
            enabled: true,
        },
        move |_, result| completion.finish(result),
    )
    .unwrap();
    wait(app, &done);
    let end = Instant::now() + Duration::from_secs(5);
    while pids(home).is_empty() {
        tick(app);
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(pids(home).len(), 1);
    disable(app, home);
}
fn shortcuts(app: &mut App, home: &Path) {
    let path = home.join("plugins.toml");
    let held = tasty_utils::file_replace::lock(&path).unwrap();
    let value = crate::plugin::registry_state::ShortcutOverride::Key {
        value: vec!["ctrl+alt+7".into()],
    };
    app.apply_plugin_shortcut_draft(crate::app::plugin_glue::shortcut::Draft::from([(
        (ID.into(), "probe".into()),
        Some(value.clone()),
    )]));
    tick(app);
    std::thread::sleep(Duration::from_millis(20));
    tick(app);
    assert!(
        app.plugin_manager
            .as_ref()
            .unwrap()
            .config
            .shortcut_override(ID, "probe")
            .is_none()
    );
    assert_ne!(app.plugin_files_pending(), 0);
    drop(held);
    let end = Instant::now() + Duration::from_secs(5);
    while app.plugin_files_pending() != 0 {
        tick(app);
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        app.plugin_manager
            .as_ref()
            .unwrap()
            .config
            .shortcut_override(ID, "probe"),
        Some(&value)
    );
    assert_eq!(
        tasty_host_plugin::PluginsConfig::load_from(path).shortcut_override(ID, "probe"),
        Some(&value)
    );
}
pub(crate) fn gui_only_consumers(make: impl FnOnce() -> App) {
    let _home = crate::test_support::IsolatedHome::new();
    let home = tasty_utils::path::tasty_home().unwrap();
    std::fs::write(
        home.join("plugins.toml"),
        format!("[disabled]\nids=[\"{ID}\"]\n"),
    )
    .unwrap();
    let dir = home.join("plugins").join(ID);
    std::fs::create_dir_all(&dir).unwrap();
    manifest(&dir);
    tasty_test_support::write_executable(&dir.join("fake.py"), FAKE).unwrap();
    let mut app = make();
    app.plugin_manager = Some(crate::plugin::PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        app.services.registries.file_format.clone(),
        app.services.registries.file_handler.clone(),
    ));
    register(&mut app, &home, &dir);
    shortcuts(&mut app, &home);
    disable(&mut app, &home);
    tracing::info!(owned_pids=?pids(&home),"L1 native App GUI register/trust/shortcut receipts; all owned PIDs reaped");
    super::shutdown_tests::cleanup(&mut app);
}
