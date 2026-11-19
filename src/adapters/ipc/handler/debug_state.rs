//! GUI feature 없이도 쓰는 디버그 UI 상태 조회와 설정 변경.
//! 팝업 상태만 GUI에서 읽고 나머지는 헤드리스에서도 처리한다.

use serde_json::json;

use crate::state::RequestContext;
use tasty_ipc::protocol::JsonRpcResponse;

pub(super) fn handle_ui_state(
    state: &RequestContext,
    engine: &crate::core::CoreState,
    id: serde_json::Value,
) -> JsonRpcResponse {
    // 창이 없는 parked engine도 조회된다. workspace가 없으면 관련 값은 0 대신 null이다.
    // active_workspace에는 system.info와 마찬가지로 저장된 인덱스를 반환한다.
    let ws = (!engine.workspaces().is_empty()).then(|| state.active_workspace(engine));
    let pane_count = ws.map(|ws| ws.pane_layout().all_pane_ids().len());
    let focused_pane = ws.and_then(|ws| {
        ws.pane_layout()
            .find_pane(state.navigation.pane_id(ws).unwrap_or(0))
    });
    let tab_count = ws.map(|_| focused_pane.map(|p| p.tabs.len()).unwrap_or(0));
    let active_tab = ws.map(|_| {
        focused_pane
            .map(|p| state.navigation.tab_index(p))
            .unwrap_or(0)
    });
    #[cfg(feature = "gui")]
    let notification_panel_open = state.popups.is_open("notifications");
    #[cfg(not(feature = "gui"))]
    let notification_panel_open = false;
    // 실제 단축키 처리와 같은 판정을 사용한다. 전체화면 무대는 오버레이보다 먼저 입력을 소비한다.
    let keyboard_shortcuts_gated = state.window_keys_to_overlay();

    // 차단 사유를 구별하도록 각 조건의 true/false도 반환한다.
    // 실제 입력 판정이 읽은 값을 그대로 보고해야 잘못된 상태도 진단할 수 있다.
    // keyboard_overlay_open의 인자와 일치하는지는 fullscreen_stage_input_gate가 검사한다.
    // 전체화면 무대는 해당 함수 밖의 조건이므로 따로 보고한다.
    #[cfg(feature = "gui")]
    let host_popup_focused = state.popups.has_focused();
    #[cfg(not(feature = "gui"))]
    let host_popup_focused = false;
    #[cfg(feature = "gui")]
    let restore_notice_focused =
        crate::adapters::ui::restore_notice::has_keyboard_focus(&state.popups);
    #[cfg(not(feature = "gui"))]
    let restore_notice_focused = false;

    // 창의 포커스 노드. 키 주인과 Esc 대상이 이 값에서 나온다.
    #[cfg(feature = "gui")]
    let focus_node = state.focus_node(engine).describe();
    #[cfg(not(feature = "gui"))]
    let focus_node = serde_json::Value::Null;

    #[cfg(feature = "gui")]
    let tutorial = json!({
        "active": state.tutorial.active.map(|a| json!({"topic": a.topic, "step": a.step})),
        "ready": state.tutorial.ready(),
        "keyboard_focus": state.tutorial.keyboard_focus,
        "catalog_open": state.popups.is_open("tutorial_topics"),
        "rect": state.tutorial.callout_rect.map(|r| [r.min.x, r.min.y, r.max.x, r.max.y]),
        "save_error": state.tutorial.save_error,
    });
    #[cfg(not(feature = "gui"))]
    let tutorial = serde_json::Value::Null;
    #[cfg(feature = "gui")]
    let attach_pending = json!({
        "selected": state.attach_pending.selected(engine, &state.navigation).map(|entry| entry.id.0),
        "focused_surface": state.focused_surface_id(engine),
        "requests": state.attach_pending.entries.iter().filter(|entry| !entry.receipt.finished() && entry.epoch.is_active()).map(|entry| json!({
            "workspace": entry.workspace, "draft": entry.id.0, "op": entry.receipt.op(),
            "creation": entry.creation.is_some(), "preview": entry.visible,
            "result_received": entry.receipt.has_result(), "elapsed_ms": entry.receipt.admitted.elapsed().as_millis(),
        })).collect::<Vec<_>>(),
        "waits": state.attach_waits.iter().map(|(workspace, times)| json!({"workspace": workspace, "count": times.len(), "oldest_ms": times.iter().map(|wait| wait.admitted).min().map(|time| time.elapsed().as_millis())})).collect::<Vec<_>>(),
    });
    #[cfg(not(feature = "gui"))]
    let attach_pending = serde_json::Value::Null;
    JsonRpcResponse::success(
        id,
        json!({
            "attach_pending": attach_pending,
            "tutorial": tutorial,
            // 열기 요청과 실제 모달 표시는 다르다. 표시 여부는 modal_open으로 확인한다.
            "settings_open_requested": state.has_settings_open_request(),
            "keyboard_shortcuts_gated": keyboard_shortcuts_gated,
            "focus_node": focus_node,
            "gate_fullscreen_stage_active": state.fullscreen_stage_active(),
            "gate_settings_open_requested": state.has_settings_open_request(),
            "gate_input_dialog_open": state.has_input_dialog_open(),
            "gate_host_popup_focused": host_popup_focused,
            "gate_restore_notice_focused": restore_notice_focused,
            "gate_plugin_popup_open": state.has_plugin_popup(),
            // 모달은 GUI App의 ViewRegistry가 소유한다. 여기서는 모달이 없는 값을 내고,
            // GUI App이 응답을 보내기 전에 활성 모달로 덮어쓴다(App::project_active_modal).
            "modal_open": false,
            "active_modal_id": serde_json::Value::Null,
            "active_modal_kind": serde_json::Value::Null,
            "notification_panel_open": notification_panel_open,
            "active_workspace": state.active_workspace_index(engine),
            "workspace_count": engine.workspaces().len(),
            "pane_count": pane_count,
            "tab_count": tab_count,
            "active_tab": active_tab,
        }),
    )
}

/// 현재 설정의 직렬화 사본에 부분 patch를 합친 뒤 전체 교체로 적용한다.
/// 실제 설정을 미리 바꾸면 후속 처리에서 이전 값과의 차이를 알 수 없으므로 사본만 수정한다.
/// 알 수 없는 키는 무시하고 잘못된 타입은 거절한다. GUI 없이도 같은 설정 변경 경로를 쓴다.
pub(super) fn handle_debug_settings_apply(
    core: &crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(patch) = params.get("settings") else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'settings' parameter");
    };
    if !patch.is_object() {
        return JsonRpcResponse::invalid_params(id, "'settings' must be a JSON object");
    }

    let mut base = match serde_json::to_value(&engine.runtime.settings) {
        Ok(v) => v,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                -32603,
                format!("failed to serialize live settings: {e}"),
            );
        }
    };
    json_deep_merge(&mut base, patch);

    // 전체 설정에 합쳤으므로 patch에서 빠진 필드가 기본값으로 바뀌지 않는다.
    let mut new_settings: tasty_settings::Settings = match serde_json::from_value(base) {
        Ok(s) => s,
        Err(e) => {
            return JsonRpcResponse::invalid_params(id, format!("invalid settings patch: {e}"));
        }
    };
    if let Err(e) = new_settings.validate_runtime() {
        return JsonRpcResponse::invalid_params(id, e);
    }
    reapply_theme_on_change(
        &engine.runtime.settings.appearance.theme,
        &mut new_settings,
        tasty_themes::apply_theme,
    );

    // 적용과 저장이 끝난 뒤 응답한다. 합친 설정 전체로 바꾸는 기존 의미는 그대로다.
    core.request_settings_change(
        id,
        crate::app::settings_change::SettingsChange::Replace(Box::new(new_settings)),
        |intent| state.dispatch_intent(intent),
        |id, _changed, _settings| JsonRpcResponse::success(id, json!({ "applied": true })),
    )
}

/// theme id가 바뀌었으면 설정 모달·상태바 토글처럼 그 테마의 색 세트와 밝기를 적용한다.
/// id만 바꾸면 UpdateSettings가 이전 theme_base로 Theme를 설치해 화면이 바뀌지 않는다.
/// 모달과 같이 색 override는 비워진다.
fn reapply_theme_on_change(
    prev_theme: &str,
    new_settings: &mut tasty_settings::Settings,
    apply: impl FnOnce(&mut tasty_settings::AppearanceSettings, &str),
) {
    if new_settings.appearance.theme != prev_theme {
        let target = new_settings.appearance.theme.clone();
        apply(&mut new_settings.appearance, &target);
    }
}

/// object는 키별로 재귀 병합하고 그 외 값은 대체한다. 지정하지 않은 중첩 필드는 유지한다.
fn json_deep_merge(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target_map), serde_json::Value::Object(patch_map)) => {
            for (k, v) in patch_map {
                json_deep_merge(
                    target_map
                        .entry(k.clone())
                        .or_insert(serde_json::Value::Null),
                    v,
                );
            }
        }
        (target_slot, patch_val) => {
            *target_slot = patch_val.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::handle_ui_state;
    use super::reapply_theme_on_change;

    fn settings_with_theme(id: &str) -> tasty_settings::Settings {
        let mut s = tasty_settings::Settings::default();
        s.appearance.theme = id.to_string();
        s
    }

    /// 다른 theme id 가 오면 그 id 로 적용 함수를 한 번 부른다.
    #[test]
    fn a_changed_theme_id_goes_through_the_theme_apply_function() {
        let mut new_settings = settings_with_theme("latte");
        let mut calls = Vec::new();
        reapply_theme_on_change("mocha", &mut new_settings, |_, id| {
            calls.push(id.to_string())
        });
        assert_eq!(calls, vec!["latte".to_string()]);
    }

    /// theme id 가 그대로면 적용하지 않는다 — 다른 설정 patch 가 색 override 를 지우지 않는다.
    #[test]
    fn an_unchanged_theme_id_leaves_the_colour_set_alone() {
        let mut new_settings = settings_with_theme("mocha");
        let mut calls = 0;
        reapply_theme_on_change("mocha", &mut new_settings, |_, _| calls += 1);
        assert_eq!(calls, 0);
    }

    /// 핸들러가 이전 theme 과 patch 의 theme 을 비교해 적용 함수를 부르는지 배선째 확인한다.
    /// 내장 mocha 로 바꾸면 theme_is_light 와 색 override 가 그 theme 값으로 정해진다.
    /// test_state 의 엔진이 이 스레드의 tasty_home 을 임시 디렉터리로 격리한다.
    #[test]
    fn settings_apply_with_a_new_theme_dispatches_that_themes_colour_set() {
        use crate::app::settings_change::test_support::{ProbeStorage, core_with, settle};
        let mut core = core_with(std::sync::Arc::new(ProbeStorage::default()));
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        {
            let appearance = &mut engine.runtime.settings.appearance;
            appearance.theme = "latte".to_string();
            appearance.theme_is_light = true;
            appearance.theme_overrides.crust =
                tasty_type_appearance::color::HexColor::from_hex("#123456");
            assert!(appearance.theme_overrides.crust.is_some());
        }

        let placeholder = super::handle_debug_settings_apply(
            &core,
            &mut state,
            &mut engine,
            serde_json::json!(1),
            &serde_json::json!({ "settings": { "appearance": { "theme": "mocha" } } }),
        );
        assert!(
            placeholder.error.is_some(),
            "the answer waits for the result"
        );

        let intents = state.take_pending_intents();
        assert_eq!(intents.len(), 1);
        let resp = settle(&mut core, &mut state, &mut engine, intents);
        assert_eq!(resp.result, Some(serde_json::json!({ "applied": true })));
        let settings = &engine.runtime.settings;
        assert_eq!(settings.appearance.theme, "mocha");
        assert!(!settings.appearance.theme_is_light);
        assert!(
            settings.appearance.theme_overrides.is_empty(),
            "{:?}",
            settings.appearance.theme_overrides
        );
    }

    /// report 의 append 상한은 블록 상한보다 작아야 한다. 어긋난 쌍은 적용 전에 거절한다.
    #[test]
    fn settings_apply_refuses_a_task_pipeline_pair_that_fails_the_check() {
        let core = crate::ipc::handler::cli_entry_tests::test_core();
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let apply = |state: &mut _, engine: &mut _, append: u64, block: u64| {
            super::handle_debug_settings_apply(
                &core,
                state,
                engine,
                serde_json::json!(1),
                &serde_json::json!({ "settings": { "task_pipeline": {
                    "report_append_bytes": append, "report_block_bytes": block } } }),
            )
        };
        let resp = apply(&mut state, &mut engine, 4096, 4096);
        assert_eq!(resp.error.expect("refused").code, -32602);
        assert!(state.take_pending_intents().is_empty());

        apply(&mut state, &mut engine, 2048, 4096);
        assert_eq!(state.take_pending_intents().len(), 1);
        // 적용 결과를 기다리는 응답을 치운다.
        drop(core.answer_parked_for_test());
    }

    #[test]
    fn anomaly_settings_patch_rejects_invalid_values_and_reaches_headless_runtime() {
        use crate::app::settings_change::test_support::{ProbeStorage, core_with, settle};
        let mut core = core_with(std::sync::Arc::new(ProbeStorage::default()));
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let response = super::handle_debug_settings_apply(
            &core,
            &mut state,
            &mut engine,
            serde_json::json!(1),
            &serde_json::json!({"settings":{"anomaly_detection":{"slow_loop_threshold":1}}}),
        );
        assert_eq!(response.error.unwrap().code, -32602);
        assert!(state.take_pending_intents().is_empty());
        let _pending = super::handle_debug_settings_apply(
            &core,
            &mut state,
            &mut engine,
            serde_json::json!(2),
            &serde_json::json!({"settings":{"anomaly_detection":{"rss_surge_min_mib":0,"rss_surge_samples":3}}}),
        );
        let intents = state.take_pending_intents();
        let response = settle(&mut core, &mut state, &mut engine, intents);
        assert_eq!(response.result, Some(serde_json::json!({"applied":true})));
        assert_eq!(
            engine.runtime.settings.anomaly_detection.rss_surge_samples,
            3
        );
        for i in 0..2 {
            assert!(
                core.anomaly_detector
                    .record_rss_sample("test", 100 + i, 1000 + i, i)
                    .is_none()
            );
        }
        assert!(
            core.anomaly_detector
                .record_rss_sample("test", 102, 1002, 2)
                .is_some()
        );
    }

    #[test]
    fn ui_state_answers_for_a_parked_engine_without_workspaces() {
        let (state, mut engine_session) =
            crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![
                tasty_core::DomainEvent::CategoryCreated {
                    id: 0,
                    name: "normal".into(),
                    index: 0,
                },
            ]));
        let engine = engine_session.borrow_mut();
        let resp = handle_ui_state(&state, &engine, serde_json::json!(1));
        let result = resp.result.expect("성공 응답이어야 한다");
        assert_eq!(result["workspace_count"], 0);
        assert!(result["pane_count"].is_null(), "{result}");
        assert!(result["tab_count"].is_null(), "{result}");
        assert!(result["active_tab"].is_null(), "{result}");
        assert_eq!(
            result["active_workspace"],
            state.active_workspace_index(&engine)
        );
        assert!(result["keyboard_shortcuts_gated"].is_boolean(), "{result}");
    }

    #[test]
    fn ui_state_keeps_the_counts_when_a_workspace_exists() {
        let (state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let resp = handle_ui_state(&state, &engine, serde_json::json!(1));
        let result = resp.result.expect("성공 응답이어야 한다");
        assert_eq!(result["workspace_count"], 1);
        assert!(
            result["pane_count"].as_u64().is_some_and(|n| n >= 1),
            "{result}"
        );
        assert!(
            result["tab_count"].as_u64().is_some_and(|n| n >= 1),
            "{result}"
        );
        assert_eq!(result["active_tab"], 0);
    }
}
