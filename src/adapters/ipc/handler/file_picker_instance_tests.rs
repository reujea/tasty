//! 파일 피커는 범위마다 하나씩 열린다. 다른 범위의 요청은 막지 않고, 결과와 목록 응답은 각자의 요청으로 간다.

use super::handle_trigger;
use crate::adapters::ui::popup::file_picker::instances;
use crate::adapters::ui::popup::{PopupScope, defs};
use crate::state::{FilePickerResult, RequestContext};
use serde_json::json;
use tasty_ipc::caller::CallerContext;

const PLUGIN: &str = "com.example.picker";

fn plugin() -> CallerContext {
    CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
    }
}

/// 두 pane 의 surface 를 (왼쪽, 오른쪽) 으로 돌려준다.
fn surfaces(engine: &crate::runtime::engine_read::EngineRead<'_>) -> (u32, u32) {
    let ids = engine.workspace_at(0).unwrap().all_surface_ids();
    assert!(ids.len() >= 2, "two panes");
    (ids[0], ids[1])
}

/// `sid` 에서 출발한 피커를 요청하고 쌓인 열기 요청을 반영한다. 응답을 돌려준다.
fn trigger_from(
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    sid: u32,
) -> tasty_ipc::protocol::JsonRpcResponse {
    let response = handle_trigger(
        state,
        engine,
        &plugin(),
        json!(1),
        &json!({ "origin_surface_id": sid, "location": "local" }),
    );
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(state, engine, &intent);
    }
    response
}

fn wire_id(response: &tasty_ipc::protocol::JsonRpcResponse) -> u64 {
    response.result.as_ref().expect("trigger succeeds")["request_id"]
        .as_u64()
        .expect("request_id")
}

/// Given R 에 피커가 열려 있을 때, When L 의 plugin surface 가 피커를 요청하면, Then busy 가 아니고 L 범위에 피커가
/// 열린다. When 둘 다 확정하면, Then 각 결과가 자기 `request_id` 의 호출자에게 간다.
#[test]
fn a_picker_on_another_surface_opens_beside_the_first_and_each_result_goes_to_its_caller() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let (left, right) = surfaces(&engine.read());
    let first = wire_id(&trigger_from(&mut state, &mut engine, right));
    let second = trigger_from(&mut state, &mut engine, left);
    let second = wire_id(&second);
    assert_ne!(first, second);

    let mut open: Vec<(PopupScope, u64)> = instances::open_pickers(&state)
        .map(|(iid, data)| {
            let scope = state.popups.instance(iid).unwrap().scope.clone();
            (scope, data.requester.as_ref().unwrap().request_id)
        })
        .collect();
    open.sort_by_key(|(_, id)| *id);
    assert_eq!(
        open,
        [
            (PopupScope::Surface(right), first),
            (PopupScope::Surface(left), second)
        ]
    );

    for picker in instances::pickers_mut(&mut state) {
        let id = picker.requester.as_ref().unwrap().request_id;
        picker.result = Some(FilePickerResult::Confirmed {
            paths: vec![format!("/picked/{id}")],
        });
    }
    let settled: Vec<_> = instances::open_pickers(&state)
        .map(|(iid, _)| iid)
        .collect();
    for iid in settled {
        state
            .popups
            .close_instance(iid, crate::adapters::ui::popup::PopupCloseReason::Content);
    }
    for closed in state.popups.take_closed_queue() {
        instances::finish(&mut state, &closed);
    }
    let mut delivered = Vec::new();
    for (outcome, requester) in crate::app::dispatch::file_picker::take_outcomes(&mut state) {
        let requester = requester.expect("plugin requester");
        assert_eq!(outcome.request_id.0, requester.request_id);
        delivered.push((requester.request_id, outcome.paths));
    }
    delivered.sort();
    assert_eq!(
        delivered,
        [
            (first, vec![format!("/picked/{first}")]),
            (second, vec![format!("/picked/{second}")])
        ]
    );
}

/// Given R 에 피커가 열려 있을 때, When R 에서 증명 없이 두 번째 피커를 요청하면, Then busy 다.
#[test]
fn a_second_picker_from_the_same_surface_is_busy() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let (_, right) = surfaces(&engine.read());
    let first = wire_id(&trigger_from(&mut state, &mut engine, right));
    let again = trigger_from(&mut state, &mut engine, right);
    assert_eq!(again.error.as_ref().map(|e| e.code), Some(-32000));
    let ids: Vec<u64> = instances::open_pickers(&state)
        .map(|(_, data)| data.requester.as_ref().unwrap().request_id)
        .collect();
    assert_eq!(ids, [first]);
}

/// 원격 목록 응답은 그 요청을 보낸 피커에만 반영한다. 같은 창의 다른 피커는 그대로 기다린다.
#[test]
fn a_listing_reply_reaches_only_the_picker_that_asked() {
    let (mut state, _session) = crate::state::tests::test_state();
    for (sid, request_id) in [(1, 10), (2, 11)] {
        let mut data = crate::state::FilePickerData {
            save_name: None,
            folder: false,
            request_id: crate::state::FilePickerRequestId(u64::from(sid)),
            mirror_ws_id: Some(9),
            remote_host: None,
            current_dir: format!("/dir/{sid}"),
            load: crate::state::FpLoadState::Empty,
            local_query: None,
            entries: Vec::new(),
            selected: Vec::new(),
            result: None,
            requester: None,
            filters: Vec::new(),
        };
        data.load = crate::state::FpLoadState::Loading {
            request_id,
            sent_at: std::time::Instant::now(),
        };
        instances::install_for_test(&mut state, PopupScope::Surface(sid), data);
    }
    let picker = instances::waiting_for_listing(&mut state, 11).expect("a picker waits");
    assert_eq!(picker.current_dir, "/dir/2");
    assert!(instances::waiting_for_listing(&mut state, 12).is_none());
}

/// 왼쪽·오른쪽 surface에 (폴더 모드) 피커를 열고 한 프레임에 둘 다 그리며 키 하나를 넣는다.
/// `keys`는 피커마다 본문이 키 주인(`PopupCx::focused`)인지다. 피커마다 그리기 결과를 돌려준다.
fn draw_two_pickers_with_key(key: egui::Key, keys: [bool; 2]) -> [&'static str; 2] {
    use crate::adapters::ui::popup::{PopupCx, PopupPayload};
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let (left, right) = surfaces(&engine.read());
    for sid in [left, right] {
        let response = handle_trigger(
            &mut state,
            &mut engine,
            &plugin(),
            json!(1),
            &json!({ "origin_surface_id": sid, "location": "local", "mode": "folder" }),
        );
        assert!(response.result.is_some(), "{response:?}");
        for intent in state.take_pending_intents() {
            crate::intent::popup::handle(&mut state, &mut engine, &intent);
        }
    }
    let mut iids: Vec<_> = instances::open_pickers(&state)
        .map(|(iid, _)| iid)
        .collect();
    iids.sort();
    assert_eq!(iids.len(), 2);
    let mut payloads: Vec<PopupPayload> = iids
        .iter()
        .map(|iid| {
            let p = state.popups.instance_mut(*iid).unwrap();
            let mut payload = std::mem::replace(&mut p.payload, PopupPayload::None);
            if let PopupPayload::FilePicker(data) = &mut payload {
                data.load = crate::state::FpLoadState::Empty;
            }
            payload
        })
        .collect();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        events: vec![egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let read = engine.read();
    let mut results = ["none"; 2];
    drop(egui::Context::default().run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            for (i, payload) in payloads.iter_mut().enumerate() {
                let area = egui::Rect::from_min_size(
                    egui::pos2(660.0 * i as f32, 0.0),
                    egui::vec2(640.0, 480.0),
                );
                ui.scope_builder(egui::UiBuilder::new().id_salt(i).max_rect(area), |ui| {
                    let mut cx = PopupCx {
                        id: crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID,
                        instance: iids[i],
                        focused: keys[i],
                        payload,
                    };
                    let action = crate::adapters::ui::popup::file_picker::draw_file_picker(
                        ui, &mut state, &read, &mut cx,
                    );
                    results[i] = match action {
                        crate::adapters::ui::PopupAction::None => "none",
                        crate::adapters::ui::PopupAction::CloseBy(
                            crate::adapters::ui::popup::PopupCloseReason::Escape,
                        ) => "escape",
                        _ => "closed",
                    };
                });
            }
        });
    }));
    results
}

/// Given 두 범위에 피커가 보이고 위의 피커가 키 주인일 때, When Escape나 폴더 모드 Enter를 누르면,
/// Then 그 피커만 반응한다. 키 주인이 없으면 어느 피커도 반응하지 않는다.
#[test]
fn only_the_key_owner_picker_reads_keys() {
    for key in [egui::Key::Escape, egui::Key::Enter] {
        let [below, above] = draw_two_pickers_with_key(key, [false, true]);
        assert_eq!(below, "none", "{key:?}");
        assert_ne!(above, "none", "{key:?}");
        if key == egui::Key::Escape {
            assert_eq!(above, "escape", "the close reason is escape");
        }
        assert_eq!(
            draw_two_pickers_with_key(key, [false, false]),
            ["none", "none"],
            "{key:?}"
        );
    }
}
