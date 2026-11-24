//! 원격 File Picker는 열 때부터 원격이다(`mirror_ws_id`). 첫 목록 응답 전의 로딩, 요청 실패·시간 초과에서도
//! 제목 배지와 원격 로딩 문구를 그린다. 호스트 글자는 열 때 위치에서 읽고, 위치가 아직 없으면(동기화 전)
//! 빈 값을 확정하지 않고 다음 프레임에 다시 읽는다.

use crate::adapters::ui::popup::file_picker::{self, FilePickerOpenedBy, FilePickerOptions};
use crate::adapters::ui::popup::{PopupCx, PopupPayload, PopupScope, defs};
use crate::i18n::t;
use crate::remote::state::MirrorLocation;
use crate::state::{FilePickerLocation, FilePickerRequestId, FpLoadState};

struct Fixture {
    state: crate::state::RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    payload: PopupPayload,
    ws: u32,
}

/// mirror workspace를 원격 위치로 피커를 연다. `location`이 Some이면 열기 전에 그 위치를 게시한다.
fn open_remote(location: Option<MirrorLocation>) -> Fixture {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_mirror_state();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let ws = {
        let mut engine = session.borrow_mut();
        let ws = engine.workspace_at(0).expect("mirror workspace").id;
        if let Some(location) = location {
            engine.remote.mirror_locations.insert(ws, location);
        }
        let read = engine.read();
        file_picker::open(
            &mut state,
            &read,
            FilePickerRequestId(1),
            FilePickerLocation::Remote {
                mirror_ws_id: ws,
                surface_id: None,
                dir: None,
            },
            FilePickerOptions::default(),
            FilePickerOpenedBy::ToolsMenu,
            PopupScope::Window,
        )
        .expect("picker opens");
        drop(read);
        for intent in state.take_pending_intents() {
            crate::intent::popup::handle(&mut state, &mut engine, &intent);
        }
        ws
    };
    let iid = file_picker::instances::open_pickers(&state)
        .map(|(iid, _)| iid)
        .next()
        .expect("a picker is open");
    let payload = std::mem::replace(
        &mut state.popups.instance_mut(iid).expect("instance").payload,
        PopupPayload::None,
    );
    Fixture {
        state,
        session,
        payload,
        ws,
    }
}

impl Fixture {
    fn data(&mut self) -> &mut crate::state::FilePickerData {
        file_picker::instances::picker_of_mut(&mut self.payload).expect("picker payload")
    }

    /// 피커를 한 프레임 그리고 칠한 글자들을 돌려준다.
    fn painted(&mut self) -> Vec<String> {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let mut engine = self.session.borrow_mut();
        let read = engine.read();
        let state = &mut self.state;
        let payload = &mut self.payload;
        let mut out = Vec::new();
        // 첫 pass에서 글꼴이 준비되므로 두 번 돈다.
        for _ in 0..2 {
            let full = ctx.run(raw.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut cx = PopupCx {
                        id: file_picker::FILE_PICKER_POPUP_ID,
                        instance: crate::adapters::ui::popup::PopupInstanceId(1),
                        focused: false,
                        keys: crate::adapters::ui::popup::keys::PopupKeys::new(
                            ui.ctx(),
                            false,
                            crate::adapters::ui::popup::PopupKeyboard::Text,
                            &crate::adapters::ui::input::edit_key::EditBindings::default(),
                        ),
                        payload,
                    };
                    let _action = file_picker::draw_file_picker(ui, state, &read, &mut cx);
                });
            });
            out = full
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
        }
        drop(read);
        drop(engine);
        out
    }
}

fn has(texts: &[String], wanted: &str) -> bool {
    texts.iter().any(|text| text == wanted)
}

/// 첫 목록 응답 전 로딩에서 제목 배지(빈 호스트면 낱말 Remote)와 원격 로딩 문구를 그린다.
#[test]
fn the_first_loading_frame_draws_the_host_badge_and_the_remote_loading_body() {
    let mut f = open_remote(Some(MirrorLocation {
        host: String::new(),
        reconnecting: false,
    }));
    assert!(matches!(f.data().load, FpLoadState::Loading { .. }));
    let texts = f.painted();
    assert!(has(&texts, t("attach.mirror_pill_label")), "{texts:?}");
    assert!(
        has(&texts, t("filepicker.loading_body_remote")),
        "{texts:?}"
    );
    assert!(
        !has(&texts, t("filepicker.loading_body_local")),
        "{texts:?}"
    );

    let mut f = open_remote(Some(MirrorLocation {
        host: "work-laptop-attach".into(),
        reconnecting: false,
    }));
    let texts = f.painted();
    assert!(has(&texts, "work-laptop-attach"), "{texts:?}");
    assert!(
        has(&texts, t("filepicker.loading_body_remote")),
        "{texts:?}"
    );
}

/// 위치가 아직 없을 때(동기화 전) 연 피커는 빈 호스트를 확정하지 않는다. 배지는 낱말 Remote로 그리고,
/// 위치가 게시되면 다음 프레임에 그 호스트로 바뀐다.
#[test]
fn a_picker_opened_before_the_location_sync_picks_up_the_host_later() {
    let mut f = open_remote(None);
    let texts = f.painted();
    assert!(has(&texts, t("attach.mirror_pill_label")), "{texts:?}");
    let ws = f.ws;
    f.session.borrow_mut().remote.mirror_locations.insert(
        ws,
        MirrorLocation {
            host: "work-laptop-attach".into(),
            reconnecting: false,
        },
    );
    let texts = f.painted();
    assert!(has(&texts, "work-laptop-attach"), "{texts:?}");
    assert!(!has(&texts, t("attach.mirror_pill_label")), "{texts:?}");
}

/// 첫 조회가 끊기거나(오류 응답) 시간을 넘겨도 배지는 남는다.
#[test]
fn the_host_badge_stays_when_the_first_listing_fails_or_times_out() {
    for timed_out in [false, true] {
        let mut f = open_remote(Some(MirrorLocation {
            host: "work-laptop-attach".into(),
            reconnecting: false,
        }));
        if timed_out {
            f.data().load = FpLoadState::Loading {
                request_id: 1,
                sent_at: std::time::Instant::now() - std::time::Duration::from_secs(3600),
            };
        } else {
            f.data().load =
                FpLoadState::ErrorConn(t("filepicker.error_conn.session_lost").to_string());
        }
        let texts = f.painted();
        assert!(
            matches!(f.data().load, FpLoadState::ErrorConn(_)),
            "timed_out={timed_out}"
        );
        assert!(
            has(&texts, "work-laptop-attach"),
            "timed_out={timed_out}: {texts:?}"
        );
        assert!(
            has(&texts, t("filepicker.error_conn.title")),
            "timed_out={timed_out}: {texts:?}"
        );
    }
}
