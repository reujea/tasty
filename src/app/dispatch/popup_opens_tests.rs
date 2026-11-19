//! 도구·명령으로 연 창 범위 plugin 팝업이 입력이 일어난 창에만 그려지는지 잰다.
//! 창 둘은 MainViewState 둘이다. 열기는 도구 메뉴와 같은 `invoke_tool_on`, 그리기는 `draw_plugin_popups`를 거친다.

use std::sync::Arc;

use egui::{Context, Pos2, Rect, Vec2};

use super::open_pending_popups;
use crate::plugin::PluginManager;
use crate::plugin::manifest::ToolAction;
use crate::plugin::tool_registry::{ToolItem, ToolSource};
use crate::state::MainViewState;

const PLUGIN: &str = "com.example.window-popup";

fn manager() -> (PluginManager, tasty_host_plugin::manager::CallerStub) {
    let mut mgr = PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let manifest = toml::from_str(&format!(
        "manifest_version = 1\nid = \"{PLUGIN}\"\nname = \"Window popup\"\nversion = \"1.0.0\"\n\
         api_version = \"{}\"\n[entry]\ntype = \"process\"\ncommand = \"echo\"\n",
        crate::plugin::manifest::HOST_API_VERSION
    ))
    .expect("manifest");
    mgr.install_packages_for_test(vec![tasty_host_plugin::PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/window-popup"),
        manifest,
    }]);
    let stub = mgr.attach_caller_stub_for_test(PLUGIN);
    let decl = serde_json::from_value::<tasty_plugin_protocol::PopupDeclSpec>(
        serde_json::json!({ "scope_kind": "window", "size": { "width": 200, "height": 100 } }),
    )
    .expect("spec")
    .to_decl();
    mgr.declare_popup(PLUGIN, "viewer", decl).expect("declare");
    (mgr, stub)
}

fn tool() -> ToolItem {
    ToolItem {
        source: ToolSource::Plugin {
            plugin_id: PLUGIN.into(),
            tool_id: "open".into(),
        },
        key: format!("{PLUGIN}/open"),
        label_i18n_key: String::new(),
        icon: None,
        action: ToolAction::OpenPopup {
            popup_id: format!("{PLUGIN}/viewer"),
        },
        order_hint: 0,
    }
}

/// 한 프레임을 그리고 이 창에 놓인 plugin 팝업 인스턴스를 돌려준다.
fn drawn(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
) -> Vec<u64> {
    let ctx = Context::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
        ..Default::default()
    };
    let _full_output = ctx.run(input, |ctx| {
        crate::plugin_bridge::popup_render::draw_plugin_popups(
            ctx,
            state,
            engine,
            Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
            None,
        );
    });
    state
        .plugin_popup_key_candidates
        .iter()
        .map(|(id, _)| *id)
        .collect()
}

#[test]
fn a_window_popup_opened_by_a_tool_is_drawn_only_in_the_window_where_it_was_entered() {
    let (mut mgr, _stub) = manager();
    let (mut a, mut a_session) = crate::state::tests::test_state();
    let (mut b, mut b_session) = crate::state::tests::test_state();
    {
        let engine = a_session.borrow_mut();
        let read = engine.read();
        let target = a.command_target(&read);
        let surface = a.focused_surface_id(&read);
        crate::adapters::ui::tools_menu::invoke_tool_on(
            &mut a,
            &read,
            &tool(),
            surface,
            target.structure_target(),
        );
    }
    assert_eq!(a.pending_popup_opens.len(), 1, "the tool queues one open");

    open_pending_popups(
        Some(&mut mgr),
        [
            (&mut a, a_session.borrow_mut()),
            (&mut b, b_session.borrow_mut()),
        ]
        .into_iter(),
    );
    let opened: Vec<u64> = mgr.popup_instances().map(|(id, _)| id).collect();
    assert_eq!(opened.len(), 1, "one instance is opened");

    let in_a = drawn(&mut a, &a_session.borrow_mut().read(), &mgr);
    let in_b = drawn(&mut b, &b_session.borrow_mut().read(), &mgr);
    assert_eq!(in_a, opened, "drawn in the window where the tool ran");
    assert!(in_b.is_empty(), "not drawn in the other window: {in_b:?}");
}

/// 이미 한 창에 열린 팝업을 다른 창에서 다시 열면 같은 인스턴스를 돌려받는다. 처음 결속한 창을 유지한다.
#[test]
fn reopening_from_another_window_keeps_the_first_window() {
    let (mut mgr, _stub) = manager();
    let (mut a, mut a_session) = crate::state::tests::test_state();
    let (mut b, mut b_session) = crate::state::tests::test_state();
    for state in [&mut a, &mut b] {
        state
            .pending_popup_opens
            .push(crate::state::PendingPopupOpen {
                plugin_id: PLUGIN.into(),
                popup_id: "viewer".into(),
                context: serde_json::Value::Null,
                target_surface: None,
            });
    }
    open_pending_popups(
        Some(&mut mgr),
        std::iter::once((&mut a, a_session.borrow_mut())),
    );
    open_pending_popups(
        Some(&mut mgr),
        std::iter::once((&mut b, b_session.borrow_mut())),
    );
    let opened: Vec<u64> = mgr.popup_instances().map(|(id, _)| id).collect();
    assert_eq!(opened.len(), 1, "one instance per plugin popup");
    assert_eq!(
        a.plugin_window_popups.iter().copied().collect::<Vec<_>>(),
        opened
    );
    assert!(
        b.plugin_window_popups.is_empty(),
        "{:?}",
        b.plugin_window_popups
    );
}

fn open_tool_in(
    state: &mut MainViewState,
    session: &mut crate::runtime::engine_session::EngineSession,
) {
    let engine = session.borrow_mut();
    let read = engine.read();
    let target = state.command_target(&read);
    let surface = state.focused_surface_id(&read);
    crate::adapters::ui::tools_menu::invoke_tool_on(
        state,
        &read,
        &tool(),
        surface,
        target.structure_target(),
    );
}

/// 창을 닫으면 그 창에 결속한 창 범위 팝업이 그 자리에서 닫힌다. 같은 회차에(닫기 큐를 비우기 전에) 남은 창에서
/// 같은 도구로 다시 열면 새 인스턴스가 그 창에 결속돼 그려진다.
#[test]
fn closing_a_window_closes_its_window_popup_and_another_window_can_open_it_again() {
    use crate::app::dispatch::plugin_popup_events::close_retired_window;
    let (mut mgr, _stub) = manager();
    let (mut a, mut a_session) = crate::state::tests::test_state();
    let (mut b, mut b_session) = crate::state::tests::test_state();
    open_tool_in(&mut a, &mut a_session);
    open_pending_popups(
        Some(&mut mgr),
        [
            (&mut a, a_session.borrow_mut()),
            (&mut b, b_session.borrow_mut()),
        ]
        .into_iter(),
    );
    let first: Vec<u64> = mgr.popup_instances().map(|(id, _)| id).collect();
    assert_eq!(first.len(), 1);

    // 창 A를 버린다. 남은 창 B의 닫기 큐는 비우지 않는다.
    close_retired_window(&mut a, Some(&mut mgr));
    assert_eq!(
        mgr.popup_instances().count(),
        0,
        "the popup of the closed window is closed at once"
    );

    open_tool_in(&mut b, &mut b_session);
    open_pending_popups(
        Some(&mut mgr),
        std::iter::once((&mut b, b_session.borrow_mut())),
    );
    let reopened: Vec<u64> = mgr.popup_instances().map(|(id, _)| id).collect();
    assert_eq!(reopened.len(), 1);
    assert_ne!(reopened, first, "a new instance, not the closing one");
    assert!(b.plugin_window_popups.contains(&reopened[0]));
    let in_b = drawn(&mut b, &b_session.borrow_mut().read(), &mgr);
    assert_eq!(in_b, reopened, "drawn in the remaining window");
}

/// 창을 닫으면 그 창 팝업의 자식 파일 피커가 취소되고 plugin에 취소 결과가 간다.
#[test]
fn closing_a_window_sends_a_cancelled_result_for_the_child_picker_of_its_popup() {
    use crate::adapters::ui::popup::PopupScope;
    use crate::app::dispatch::plugin_popup_events::close_retired_window;
    let (mut mgr, stub) = manager();
    let (mut a, mut a_session) = crate::state::tests::test_state();
    open_tool_in(&mut a, &mut a_session);
    open_pending_popups(
        Some(&mut mgr),
        std::iter::once((&mut a, a_session.borrow_mut())),
    );
    let owner = mgr
        .popup_instances()
        .map(|(id, _)| id)
        .next()
        .expect("opened");
    crate::adapters::ui::popup::file_picker::instances::install_for_test(
        &mut a,
        PopupScope::Window,
        crate::state::FilePickerData {
            save_name: None,
            folder: false,
            request_id: crate::state::FilePickerRequestId(1),
            mirror_ws_id: None,
            remote_host: None,
            current_dir: "/tmp".to_string(),
            load: crate::state::FpLoadState::Empty,
            local_query: None,
            entries: Vec::new(),
            selected: Vec::new(),
            result: None,
            requester: Some(crate::state::FilePickerRequester {
                plugin_id: PLUGIN.to_string(),
                request_id: 42,
                owner_popup_instance: Some(owner),
                origin_surface_id: None,
            }),
            filters: Vec::new(),
        },
    );
    drop(stub.drain_requests());

    close_retired_window(&mut a, Some(&mut mgr));

    let sent = stub.drain_requests();
    let result = sent
        .iter()
        .find(|(_, params)| params.to_string().contains("file_picker.result"))
        .unwrap_or_else(|| panic!("file_picker.result sent: {sent:?}"));
    let text = result.1.to_string();
    assert!(text.contains("\"cancelled\":true"), "{text}");
    assert!(text.contains("\"request_id\":42"), "{text}");
}
