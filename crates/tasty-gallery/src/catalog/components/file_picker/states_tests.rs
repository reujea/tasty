//! File Picker 상태 표본의 카드마다 로딩·연결 끊김 본문이 그 상태의 앱 키 값인지 본다.
use super::indicator::NO_PROFILE_STATE_CARDS;
use super::{FpState, STATE_CARDS, Variant, card};
use crate::i18n::t;

/// 카드 한 장을 그리고 칠한 글자를 모은다.
fn painted(v: Variant) -> Vec<String> {
    let theme = crate::host_shell::latte_theme();
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    // 첫 프레임은 폰트가 확정되지 않아 galley 가 비는 경우가 있어 두 번 돈다.
    for _ in 0..2 {
        let full = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| card(ui, &theme, v));
        });
        texts.clear();
        for clipped in &full.shapes {
            collect(&clipped.shape, &mut texts);
        }
    }
    texts
}

fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
    match shape {
        egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_owned()),
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
        _ => {}
    }
}

/// 표본 표에서 묶음 이름으로 카드를 찾아 표본이 그리는 변형 그대로 돌려준다.
fn card_named(cards: &[(&'static str, Variant)], label: &str) -> Variant {
    cards
        .iter()
        .find(|(l, _)| *l == label)
        .unwrap_or_else(|| panic!("no state card {label}"))
        .1
}

fn state_card(label: &str) -> Variant {
    card_named(&STATE_CARDS, label)
}

const NO_PROFILE_VIEW: &str = "View · file-picker-remote-states-no-profile — ";

fn no_profile_card(rest: &str) -> Variant {
    card_named(&NO_PROFILE_STATE_CARDS, &format!("{NO_PROFILE_VIEW}{rest}"))
}

/// 본문 자리에 `want` 키 값이 있고 같은 블록의 다른 본문 키 값과 갤러리 전용 문장은 없다.
fn assert_body(label: &str, texts: &[String], want: &[&'static str], not: &[&'static str]) {
    for key in want {
        assert!(
            texts.iter().any(|s| s == t(key)),
            "{label}: {key} = {:?} 가 없다: {texts:?}",
            t(key)
        );
    }
    for key in not {
        assert!(
            !texts.iter().any(|s| s == t(key)),
            "{label}: {key} 를 그렸다: {texts:?}"
        );
    }
    let gallery_only: Vec<&String> = texts
        .iter()
        .filter(|s| {
            s.contains("over SSH") || s.contains("SSH tunnel") || s.contains("Loading folder")
        })
        .collect();
    assert!(gallery_only.is_empty(), "{label}: {gallery_only:?}");
}

#[test]
fn each_remote_state_card_draws_the_app_body_of_its_state() {
    let loading = state_card("loading (remote)");
    assert!(!loading.host.is_empty() && loading.state == FpState::Loading);
    assert_body(
        "named loading",
        &painted(loading),
        &["filepicker.loading", "filepicker.loading_body_remote"],
        &["filepicker.loading_body_local"],
    );

    let lost = state_card("connection lost (remote)");
    assert!(!lost.host.is_empty() && lost.state == FpState::ErrorConn);
    assert_body(
        "named session lost",
        &painted(lost),
        &[
            "filepicker.error_conn.title",
            "filepicker.error_conn.session_lost",
            "filepicker.error_conn.reconnect",
        ],
        &["filepicker.error_conn.timeout"],
    );

    assert_body(
        "no-profile loading",
        &painted(no_profile_card("loading (remote, no profile name)")),
        &["filepicker.loading", "filepicker.loading_body_remote"],
        &["filepicker.loading_body_local"],
    );
    assert_body(
        "no-profile session lost",
        &painted(no_profile_card(
            "connection lost — session lost (remote, no profile name)",
        )),
        &[
            "filepicker.error_conn.title",
            "filepicker.error_conn.session_lost",
        ],
        &["filepicker.error_conn.timeout"],
    );
    assert_body(
        "no-profile timeout",
        &painted(no_profile_card(
            "connection lost — timeout (remote, no profile name)",
        )),
        &[
            "filepicker.error_conn.title",
            "filepicker.error_conn.timeout",
        ],
        &["filepicker.error_conn.session_lost"],
    );
}
