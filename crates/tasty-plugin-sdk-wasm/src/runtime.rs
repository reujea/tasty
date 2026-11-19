//! wasmtime의 component API로 컴포넌트를 읽고 export를 호출한다.

use std::path::Path;
use std::sync::Arc;

use wasmtime::Result;
use wasmtime::component::{Component, Linker};
use wasmtime::error::Context;
use wasmtime::{Config, Engine, Store};

use crate::bridge::HostBridge;
use crate::store_state::HostState;

pub struct WasmPluginRuntime {
    engine: Engine,
    store: Store<HostState>,
    instance: wasmtime::component::Instance,
}

impl WasmPluginRuntime {
    pub fn load(
        component_path: impl AsRef<Path>,
        bridge: Arc<dyn HostBridge + Send + Sync>,
    ) -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config).context("wasmtime engine create")?;

        let component = Component::from_file(&engine, component_path.as_ref())
            .with_context(|| format!("load component {}", component_path.as_ref().display()))?;

        let mut linker: Linker<HostState> = Linker::new(&engine);

        // WASI Preview 2 import를 등록한다. 접근 자원은 HostState의 WasiCtx 설정을 따른다.
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).context("wasi-p2 add_to_linker")?;

        install_tasty_host_imports(&mut linker)?;

        let mut store = Store::new(&engine, HostState::new(bridge));
        let instance = linker
            .instantiate(&mut store, &component)
            .context("instantiate component")?;

        Ok(Self {
            engine,
            store,
            instance,
        })
    }

    /// init(plugin-id, locale) export 호출.
    pub fn init(&mut self, plugin_id: &str, locale: &str) -> Result<()> {
        let func = self
            .lifecycle_export("init")
            .context("lookup export 'init'")?;
        let typed = func
            .typed::<(String, String), ()>(&mut self.store)
            .context("typed export 'init'")?;
        typed
            .call(&mut self.store, (plugin_id.into(), locale.into()))
            .context("call init")?;
        Ok(())
    }

    pub fn open_popup(&mut self, ctx_json: &str) -> Result<String> {
        self.call_json("open-popup", ctx_json)
    }

    pub fn handle_popup_event(&mut self, ctx_json: &str) -> Result<String> {
        self.call_json("handle-popup-event", ctx_json)
    }

    fn call_json(&mut self, name: &str, arg: &str) -> Result<String> {
        let func = self
            .lifecycle_export(name)
            .with_context(|| format!("lookup export '{name}'"))?;
        let typed = func
            .typed::<(String,), (String,)>(&mut self.store)
            .with_context(|| format!("typed export '{name}'"))?;
        let (out,) = typed
            .call(&mut self.store, (arg.into(),))
            .with_context(|| format!("call {name}"))?;
        Ok(out)
    }

    fn lifecycle_export(&mut self, name: &str) -> Result<wasmtime::component::Func> {
        // WIT: world tasty-plugin { export lifecycle; }
        // → exports 의 instance 이름 = "tasty:plugin/lifecycle@0.1.0-poc".
        let iface_idx = self
            .instance
            .get_export_index(&mut self.store, None, "tasty:plugin/lifecycle@0.1.0-poc")
            .context("lookup lifecycle export instance index")?;
        let func_idx = self
            .instance
            .get_export_index(&mut self.store, Some(&iface_idx), name)
            .with_context(|| format!("lookup func index '{name}'"))?;
        self.instance
            .get_func(&mut self.store, func_idx)
            .with_context(|| format!("lookup func '{name}'"))
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

fn install_tasty_host_imports(linker: &mut Linker<HostState>) -> Result<()> {
    // WIT: package tasty:plugin@0.1.0-poc; interface host { ... }
    // → wasm component imports 의 instance 이름 = "tasty:plugin/host@0.1.0-poc".
    let mut iface = linker
        .instance("tasty:plugin/host@0.1.0-poc")
        .context("register tasty:plugin/host instance")?;

    // host-call(method: string, params-json: string) -> result<string, string>
    iface.func_wrap("host-call", |store, (method, params): (String, String)| {
        let bridge = store.data().bridge.clone();
        let res = bridge.host_call(&method, &params);
        Ok((res,))
    })?;
    // log(level: string, msg: string)
    iface.func_wrap("log", |store, (level, msg): (String, String)| {
        store.data().bridge.log(&level, &msg);
        Ok(())
    })?;
    // tr(key: string, locale: string) -> string
    iface.func_wrap("tr", |store, (key, locale): (String, String)| {
        let out = store.data().bridge.tr(&key, &locale);
        Ok((out,))
    })?;
    Ok(())
}
