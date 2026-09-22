//! Tab 도메인 Intent 핸들러.
//!
//! 정책:
//! - **NewTab**: `DomainIntent::CreateTab` 으로 forward. focused pane 의 id 는
//!   handler 안에서 결정 (`state.active_workspace(engine).focused_pane`).
//!   terminal kind 면 cwd 도 handler 가 inherit 결정. 새 탭은 **사용자가 발화했을 때만
//!   선택한다** — 에이전트 라벨로 온 것은 background 로 붙는다(ADR-0502 · ADR-0526).
//!   에이전트 표면(`tab.create`)은 이 인텐트를 안 거치고 background 로 붙인다.

use super::{DispatchedIntent, Intent, IntentOrigin};
use crate::core::Core;
use crate::core::CoreState;
use crate::state::AppState;

pub fn handle(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    intent: &DispatchedIntent,
) {
    if let Intent::NewTab { kind, params } = &intent.body {
        new_tab(core, state, engine, kind.as_deref(), params, &intent.origin)
    }
}

fn new_tab(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    kind: Option<&str>,
    params: &serde_json::Value,
    origin: &IntentOrigin,
) {
    let kind = kind.unwrap_or("terminal");
    let pane_id = state.active_workspace(engine).focused_pane;
    let cwd = if kind == "terminal" {
        state.resolve_inherit_cwd(engine)
    } else {
        None
    };
    let surface_params = if params.is_null() {
        serde_json::json!({})
    } else {
        params.clone()
    };

    // 파일 열기는 최근 목록에 기록(진입점 수렴 — 파일-열기 팝업·CLI 등). kind 하드코딩
    // 없이 매니페스트 `records_recent` 를 선언한 kind 만 기록(generic per-kind).
    if engine
        .surface_registry
        .get(kind)
        .is_some_and(|d| d.records_recent)
    {
        state.record_recent(kind, &surface_params);
    }

    let intent = crate::core::intent::DomainIntent::CreateTab {
        pane_id,
        cwd,
        kind: kind.to_string(),
        name: None,
        surface_params,
        // 발화 주체로 가른다 — 사용자가 연 탭은 선택하고, 에이전트 라벨(origin 없는
        // `file_handler.dispatch`)로 온 탭은 사용자가 보던 탭을 바꾸지 않는다(ADR-0502).
        // markdown 파일열기 팝업은 자기 popup 을 실어 보내 사용자로 도착한다(ADR-0526).
        activate: origin.is_user(),
    };
    match core.apply(engine, intent) {
        Ok(events) => {
            #[cfg(feature = "gui")]
            if origin.is_user() {
                for event in &events {
                    if let crate::core::intent::CoreEvent::TabCreated {
                        pane_id, tab_id, ..
                    } = event
                    {
                        state.observe_tutorial_tab_created(engine, *pane_id, *tab_id);
                    }
                }
            }
            // Headless builds have no tutorial observer; Core already applied the mutation.
            let _ = events;
        }
        Err(e) => {
            crate::core::mark_last_forward_user_triggered(engine, &e, origin);
            super::report_apply_error(state, engine, origin, &format!("NewTab kind={kind}"), &e);
        }
    }
}
