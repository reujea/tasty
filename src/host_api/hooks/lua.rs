//! Lua 이벤트와 등록 스크립트 자동실행을 연결한다.

pub(crate) struct AutofireCtx<'a> {
    pub scripts: &'a tasty_settings::ScriptRegistry,
    pub guard: &'a mut super::autofire::AutofireGuard,
    pub runs: &'a mut super::lua_runs::LuaRuns,
}

/// Lua job을 보내는 통로. `tasty.tree()`는 마지막 발행값을 읽고 메인은 job이 없는 동안 발행하지 않으므로,
/// job을 보내기 직전에 그 시점의 트리를 발행한다. 같은 통로로 여러 job을 보내면 처음 한 번만 발행한다.
/// 상태가 바뀌기 전에 보내는 한 묶음(같은 처리 단계의 이벤트들)에만 쓴다.
pub(crate) struct LuaSend<'a> {
    engine: &'a tasty_lua::LuaEngine,
    /// 없으면 발행하지 않는다. headless는 트리를 발행하지 않아 `tasty.tree()`가 늘 빈 배열이다.
    tree: Option<Box<dyn Fn() -> tasty_lua::LuaSnapshot + 'a>>,
    published: std::cell::Cell<bool>,
}

impl<'a> LuaSend<'a> {
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn new(
        engine: &'a tasty_lua::LuaEngine,
        tree: impl Fn() -> tasty_lua::LuaSnapshot + 'a,
    ) -> Self {
        Self {
            engine,
            tree: Some(Box::new(tree)),
            published: std::cell::Cell::new(false),
        }
    }

    #[cfg(not(feature = "gui"))]
    pub(crate) fn without_snapshot(engine: &'a tasty_lua::LuaEngine) -> Self {
        Self {
            engine,
            tree: None,
            published: std::cell::Cell::new(true),
        }
    }

    /// job을 보낼 엔진. 처음 부를 때 트리를 발행하므로 보내기 직전에만 부른다.
    pub(crate) fn engine(&self) -> &'a tasty_lua::LuaEngine {
        if !self.published.replace(true)
            && let Some(tree) = &self.tree
        {
            self.engine.publish_snapshot(tree());
        }
        self.engine
    }
}

/// observe-hook을 먼저 요청하고 자동실행을 따로 요청한다.
/// payload 직렬화 실패는 로그에 남기며 그 경우에도 자동실행은 계속 판단한다.
/// 엔진이 있으면 Fire job을 보내므로 먼저 트리를 발행한다.
pub(crate) fn fire<T: serde::Serialize>(
    lua: Option<&LuaSend<'_>>,
    autofire: AutofireCtx<'_>,
    event: &str,
    payload: &T,
) {
    let lua = lua.map(LuaSend::engine);
    if let Some(lua) = lua {
        match serde_json::to_value(payload) {
            Ok(v) => lua.fire(event, &v),
            Err(e) => {
                tracing::warn!(target: "tasty_lua", "fire '{event}' serialize failed: {e}")
            }
        }
    }
    for notice in
        super::autofire::dispatch(lua, autofire.scripts, autofire.guard, autofire.runs, event)
    {
        crate::system_notice::post(notice);
    }
}

/// worker 초기화 실패는 로그 후 None이다. 호스트 부팅을 여기서 중단하지 않는다.
pub(crate) fn init_engine() -> Option<tasty_lua::LuaEngine> {
    match tasty_lua::LuaEngine::new() {
        Ok(e) => Some(e),
        Err(e) => {
            tracing::warn!("lua engine init failed: {e}");
            None
        }
    }
}

#[cfg(test)]
#[path = "lua_tests.rs"]
mod tests;
