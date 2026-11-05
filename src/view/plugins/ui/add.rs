use crate::i18n::{t, t_count};
use crate::theme;

use tasty_ui_widgets::{
    PLUGIN_ADD_INSET, PluginAddBarClicks, PluginAddBarView, PluginAddPickerView,
    PluginManifestCardView, PluginTrustKind, plugin_add_bar, plugin_add_empty_hint,
    plugin_add_path_picker, plugin_add_read_error, plugin_manifest_card, plugin_trust_box, vspace,
};

use super::attention::{fingerprint_line, unverified_cause_key};
use super::{AddError, AddPreview, AddTrustState, PluginsAction, PluginsSnapshot, PluginsUiState};

pub(super) fn draw_add_tab(
    ctx: &egui::Context,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
) {
    let th = theme::theme();

    egui::CentralPanel::default().show(ctx, |ui| {
        vspace(ui, th.spacing_md);
        draw_add_form(ui, snapshot, ui_state, actions, &th);
    });
}

/// 경로 선택 블록 아래에 안내 상자 또는 매니페스트 카드와 신뢰 판정 상자를 두고,
/// 맨 아래 액션 바에서 추가하거나 취소한다. 경로를 고치면 확인한 매니페스트를 버린다.
fn draw_add_form(
    ui: &mut egui::Ui,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
    th: &theme::Theme,
) {
    // 액션 바가 창 안에 남도록, 같은 내용을 보이지 않게 먼저 그려 높이를 잰다.
    let footer_height = {
        let mut probe = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(ui.available_rect_before_wrap())
                .sizing_pass()
                .invisible(),
        );
        draw_add_footer(
            &mut probe,
            ui_state.add_preview.as_ref(),
            ui_state.add_in_flight,
            false,
        );
        probe.min_rect().height()
    };
    let scroll_height =
        (ui.available_height() - footer_height - ui.spacing().item_spacing.y).max(0.0);
    // 카드와 신뢰 상자가 같은 폭을 쓰도록 열 폭을 스크롤 영역 밖에서 정한다.
    let column_width = th.measure_xl.value().min(ui.available_width());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(scroll_height)
        .drag_to_scroll(false)
        .show(ui, |ui| {
            ui.set_width(column_width);
            ui.spacing_mut().item_spacing.y = th.spacing_lg.value();
            draw_path_picker(ui, snapshot, ui_state, th);
            if let Some(error) = &ui_state.add_error {
                plugin_add_read_error(ui, th, t(error.title_key()), error.reason());
            } else if let Some(preview) = &ui_state.add_preview {
                draw_preview(ui, preview, th, actions);
            } else {
                let template = t("plugins.add_empty_hint");
                let (before, after) = template.split_once("{}").unwrap_or((template, ""));
                plugin_add_empty_hint(ui, th, before, t("plugins.add_confirm_path"), after);
            }
        });

    let preview = ui_state.add_preview.clone();
    let focus_cancel = std::mem::take(&mut ui_state.add_focus_pending);
    let footer = draw_add_footer(ui, preview.as_ref(), ui_state.add_in_flight, focus_cancel);
    if footer.add
        && !ui_state.add_in_flight
        && let Some(preview) = &preview
    {
        actions.push(add_action(preview));
        ui_state.add_in_flight = true;
    }
    // 미인증 판정에서는 Esc 가 Cancel 과 같다. 아무것도 설치하지 않고 경로 입력으로 돌아간다.
    let esc_cancels = !ui_state.folder_chooser.is_open()
        && preview
            .as_ref()
            .is_some_and(|p| p.trust_state.is_unverified())
        && !ui_state.add_in_flight
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if (footer.cancel || esc_cancels) && !ui_state.add_in_flight {
        reset_add_state(ui_state);
    }
}

/// 추가 버튼이 보낼 동작. 미인증은 사용자의 명시적 선택으로 보낸다.
fn add_action(preview: &AddPreview) -> PluginsAction {
    let src_path = preview.src_path.clone();
    let manifest_sha256 = preview.manifest_sha256.clone();
    match &preview.trust_state {
        AddTrustState::Trusted => PluginsAction::Install {
            src_path,
            manifest_sha256,
        },
        AddTrustState::PermissionsChanged { pubkey_b64, .. }
        | AddTrustState::UnknownKey { pubkey_b64, .. } => PluginsAction::TrustAndInstall {
            src_path,
            pubkey_b64: pubkey_b64.clone(),
            manifest_sha256,
        },
        AddTrustState::MissingPubkey { .. }
        | AddTrustState::Unsigned
        | AddTrustState::SignatureError => PluginsAction::InstallUnverified {
            src_path,
            manifest_sha256,
        },
    }
}

/// 경로 입력 · 폴더 찾기 · Verify 와 설명 문단.
fn draw_path_picker(
    ui: &mut egui::Ui,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    th: &theme::Theme,
) {
    let picker = plugin_add_path_picker(
        ui,
        th,
        &PluginAddPickerView {
            label: t("plugins.add_path_label"),
            placeholder: t("plugins.add_path_placeholder"),
            find: t("plugins.add_browse"),
            verify: t("plugins.add_confirm_path"),
            verify_enabled: !ui_state.add_path_input.trim().is_empty(),
            help: t("plugins.add_help"),
        },
        &mut ui_state.add_path_input,
    );
    if picker.changed {
        ui_state.add_preview = None;
        ui_state.add_error = None;
    }
    if picker.find {
        ui_state.folder_chooser.open(
            "plugins.add",
            crate::view::file_chooser::FileChooserMode::Folder,
            &ui_state.add_path_input,
        );
    }

    if picker.verify {
        try_validate_path(ui_state, snapshot);
    }
}

/// 확인한 매니페스트의 카드와 신뢰 판정 상자.
fn draw_preview(
    ui: &mut egui::Ui,
    preview: &AddPreview,
    th: &theme::Theme,
    actions: &mut Vec<PluginsAction>,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = PLUGIN_ADD_INSET.value();
        let card = plugin_manifest_card(
            ui,
            th,
            &PluginManifestCardView {
                name: &preview.name,
                version: &preview.version,
                id: &preview.id,
                authors: &preview.authors,
                description: &preview.description,
                permissions_label: t("plugins.permissions"),
                permissions: &preview.permissions,
                surface_kinds_label: t("plugins.surface_kinds"),
                surface_kinds: &preview.surface_kinds,
                source_label: t("plugins.add_source_path"),
                source: &preview.src_path,
                homepage_label: t("plugins.homepage"),
                homepage: &preview.homepage,
                none: t("plugins.add_none"),
            },
        );
        if card.open_homepage {
            actions.push(PluginsAction::OpenHomepage {
                url: preview.homepage.clone(),
            });
        }
        draw_trust_box(ui, &preview.trust_state, th);
    });
}

/// 하단 액션 바. 높이 측정과 실제 그리기가 같은 함수를 쓴다.
/// 매니페스트가 없으면 Cancel 만 둔다. 추가할 수 없으면 추가 버튼을 disabled로 두고 이유를,
/// 아니면 부여할 권한 수를 왼쪽에 적는다. 미인증이면 추가 버튼이 danger 다.
fn draw_add_footer(
    ui: &mut egui::Ui,
    preview: Option<&AddPreview>,
    busy: bool,
    focus_cancel: bool,
) -> PluginAddBarClicks {
    let th = theme::theme();
    let blocked_key = preview.and_then(add_blocked_reason_key);
    let left = match (preview, blocked_key) {
        (None, _) => String::new(),
        (Some(_), Some(key)) => t(key).to_owned(),
        (Some(preview), None) => grants_label(preview.permissions.len()),
    };
    let add = preview.map(|p| t(add_button_key(p, blocked_key)));
    plugin_add_bar(
        ui,
        &th,
        &PluginAddBarView {
            left: &left,
            cancel: t("button.cancel"),
            add,
            add_enabled: blocked_key.is_none(),
            danger: blocked_key.is_none() && preview.is_some_and(|p| p.trust_state.is_unverified()),
            busy,
            progress: t("plugins.add_adding"),
            focus_cancel,
        },
    )
}

/// 부여할 권한 수 문구.
fn grants_label(count: usize) -> String {
    match count {
        0 => t("plugins.add_grants_none").to_owned(),
        n => t_count("plugins.add_grants", n as u64, &[&n.to_string()]),
    }
}

/// 추가 버튼 키. 키나 새 권한 묶음을 신뢰하게 되면 그 사실을, 미인증이면 "그래도"를 버튼에 적는다.
fn add_button_key(preview: &AddPreview, blocked_key: Option<&'static str>) -> &'static str {
    if blocked_key.is_some() {
        return "plugins.add_button";
    }
    match preview.trust_state {
        AddTrustState::Trusted => "plugins.add_button",
        AddTrustState::PermissionsChanged { .. } => "plugins.add_trust_and_add",
        AddTrustState::UnknownKey { .. } => "plugins.add_trust_key_anyway",
        AddTrustState::MissingPubkey { .. }
        | AddTrustState::Unsigned
        | AddTrustState::SignatureError => "plugins.add_anyway",
    }
}

/// 추가할 수 없는 매니페스트의 이유 키. 이미 설치된 경우뿐이며, 추가할 수 있으면 `None`.
fn add_blocked_reason_key(preview: &AddPreview) -> Option<&'static str> {
    preview
        .already_installed
        .then_some("plugins.add_blocked_installed")
}

/// 신뢰 상태의 상자 종류와 fingerprint.
fn trust_kind(state: &AddTrustState) -> (PluginTrustKind, Option<&str>) {
    match state {
        AddTrustState::Trusted => (PluginTrustKind::Trusted, None),
        AddTrustState::PermissionsChanged { fingerprint, .. } => {
            (PluginTrustKind::PermissionsChanged, Some(fingerprint))
        }
        AddTrustState::UnknownKey { fingerprint, .. } => {
            (PluginTrustKind::UnknownKey, Some(fingerprint))
        }
        AddTrustState::MissingPubkey { fingerprint } => {
            (PluginTrustKind::MissingPubkey, Some(fingerprint))
        }
        AddTrustState::Unsigned => (PluginTrustKind::Unsigned, None),
        AddTrustState::SignatureError => (PluginTrustKind::SignatureError, None),
    }
}

/// 상자의 제목과 본문 문단. 미인증 네 가지는 같은 제목·위험 문단이고 원인 문장만 다르다.
/// 모르는 키는 추가하면 그 키도 신뢰한다는 문장을 위험 문단 끝에 붙인다.
fn trust_copy(kind: PluginTrustKind) -> (&'static str, Vec<String>) {
    match kind {
        PluginTrustKind::Trusted => ("", vec![t("plugins.trust_trusted_body").to_owned()]),
        PluginTrustKind::PermissionsChanged => (
            t("plugins.trust_permissions_changed_title"),
            vec![t("plugins.trust_permissions_changed_body").to_owned()],
        ),
        _ => {
            let mut risk = t("plugins.trust_unverified_risk").to_owned();
            if kind == PluginTrustKind::UnknownKey {
                risk.push(' ');
                risk.push_str(t("plugins.trust_key_note"));
            }
            (
                t("plugins.trust_unverified_title"),
                vec![t(unverified_cause_key(kind)).to_owned(), risk],
            )
        }
    }
}

/// 매니페스트 카드 아래의 신뢰 판정 상자. 이미 설치된 플러그인도 판정대로 그린다.
fn draw_trust_box(ui: &mut egui::Ui, state: &AddTrustState, th: &theme::Theme) {
    let (kind, fingerprint) = trust_kind(state);
    let (title, body) = trust_copy(kind);
    let body: Vec<&str> = body.iter().map(String::as_str).collect();
    plugin_trust_box(ui, th, kind, title, &body, |ui| {
        if let Some(fp) = fingerprint {
            fingerprint_line(ui, th, fp);
        }
    });
}

/// `Add` 탭의 상태를 초기 입력 화면으로 되돌린다.
fn reset_add_state(ui_state: &mut PluginsUiState) {
    ui_state.add_preview = None;
    ui_state.add_error = None;
    ui_state.add_path_input.clear();
    ui_state.add_focus_pending = false;
}

/// 입력 경로로 매니페스트를 로드하고 preview/에러를 채운다.
fn try_validate_path(ui_state: &mut PluginsUiState, snapshot: &PluginsSnapshot) {
    let raw = ui_state.add_path_input.trim().to_string();
    ui_state.add_error = None;
    ui_state.add_preview = None;
    if raw.is_empty() {
        return;
    }
    let path = std::path::PathBuf::from(&raw);
    // 보일 내용을 읽기 전에 digest 를 잡는다. 그 사이 파일이 바뀌면 설치가 거절되고, 보이지 않은
    // 내용이 이 digest 로 승인되는 일은 없다. 읽지 못하면 빈 값이라 설치가 거절된다.
    let manifest_sha256 =
        tasty_host_plugin::trust_verdict::manifest_sha256(&path).unwrap_or_default();
    // 읽기·파싱 실패와 선언 검사 실패는 상자 제목이 다르다.
    let checked = match crate::plugin::Manifest::read(&path) {
        Err(e) => Err(AddError::Read(e.to_string())),
        Ok(m) => m
            .validate()
            .and_then(|()| crate::plugin_bridge::manifest_validate::validate_bin_extras(&m))
            .map(|()| m)
            .map_err(|e| AddError::Invalid(e.to_string())),
    };
    match checked {
        Ok(manifest) => {
            let already = snapshot.plugins.iter().any(|p| p.id == manifest.id);
            let trust_state = compute_trust_state(&path, &manifest.id);
            // 미인증 판정을 처음 보이면 Cancel 이 포커스를 받는다. Enter 로 설치되지 않는다.
            ui_state.add_focus_pending = trust_state.is_unverified();
            ui_state.add_preview = Some(AddPreview {
                src_path: path.to_string_lossy().to_string(),
                id: manifest.id.clone(),
                name: manifest.name,
                version: manifest.version,
                description: manifest.description,
                authors: manifest.authors,
                homepage: manifest.homepage,
                surface_kinds: manifest
                    .surface_kinds
                    .iter()
                    .map(|k| k.kind.clone())
                    .collect(),
                permissions: manifest.permissions,
                already_installed: already,
                trust_state,
                manifest_sha256,
            });
        }
        Err(error) => {
            // 오류 원문은 번역하지 않고 상자의 둘째 줄에 그대로 보인다.
            ui_state.add_error = Some(error);
        }
    }
}

/// 설치·부팅과 같은 서명 판정을 UI 상태로 바꾼다. 사용자 선택 기록은 보지 않는다.
fn compute_trust_state(dir: &std::path::Path, plugin_id: &str) -> AddTrustState {
    use tasty_host_plugin::known_plugins::{KnownPluginEntry, KnownPlugins};
    use tasty_host_plugin::trust_verdict::{TrustVerdict, UnverifiedCause, judge_signature};

    match judge_signature(dir) {
        TrustVerdict::Trusted | TrustVerdict::AcceptedUnverified => AddTrustState::Trusted,
        TrustVerdict::PermissionsChanged { fingerprint, .. } => {
            let key = KnownPlugins::load()
                .ok()
                .and_then(|db| db.lookup(plugin_id).map(|e| e.pubkey.clone()));
            match key {
                Some(pubkey_b64) => AddTrustState::PermissionsChanged {
                    fingerprint,
                    pubkey_b64,
                },
                None => {
                    tracing::warn!(plugin_id, "trusted key vanished while verifying");
                    AddTrustState::SignatureError
                }
            }
        }
        TrustVerdict::Unverified(UnverifiedCause::UnknownKey {
            fingerprint,
            pubkey,
        }) => AddTrustState::UnknownKey {
            fingerprint,
            pubkey_b64: KnownPluginEntry::encode_pubkey(&pubkey),
        },
        TrustVerdict::Unverified(UnverifiedCause::MissingPubkey { fingerprint }) => {
            AddTrustState::MissingPubkey { fingerprint }
        }
        TrustVerdict::Unverified(UnverifiedCause::Unsigned) => AddTrustState::Unsigned,
        TrustVerdict::Unverified(cause @ UnverifiedCause::SignatureError { .. }) => {
            // 신뢰 상자는 고정 문구만 보이므로 검증 실패의 원인은 로그로 남긴다.
            tracing::warn!(dir = %dir.display(), error = %cause.describe(), "plugin signature check failed");
            AddTrustState::SignatureError
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::plugins::ui::text_probe::visible_text_rects;

    fn preview(trust_state: AddTrustState, already_installed: bool) -> AddPreview {
        AddPreview {
            src_path: "/tmp/p".into(),
            id: "com.example.p".into(),
            name: "p".into(),
            version: "0.1.0".into(),
            description: String::new(),
            authors: Vec::new(),
            homepage: String::new(),
            surface_kinds: Vec::new(),
            permissions: Vec::new(),
            already_installed,
            trust_state,
            manifest_sha256: "digest".into(),
        }
    }

    fn unverified_states() -> [AddTrustState; 4] {
        [
            AddTrustState::UnknownKey {
                fingerprint: "16:43:83:e3".into(),
                pubkey_b64: "k".into(),
            },
            AddTrustState::MissingPubkey {
                fingerprint: "16:43:83:e3:a7:6d:5c:20".into(),
            },
            AddTrustState::Unsigned,
            AddTrustState::SignatureError,
        ]
    }

    #[test]
    fn only_an_already_installed_plugin_blocks_adding() {
        let mut states = unverified_states().to_vec();
        states.push(AddTrustState::Trusted);
        states.push(AddTrustState::PermissionsChanged {
            fingerprint: "fp".into(),
            pubkey_b64: "k".into(),
        });
        for state in states {
            assert_eq!(
                add_blocked_reason_key(&preview(state.clone(), false)),
                None,
                "{state:?}"
            );
            assert_eq!(
                add_blocked_reason_key(&preview(state.clone(), true)),
                Some("plugins.add_blocked_installed"),
                "{state:?}"
            );
        }
    }

    /// 미인증 네 가지는 모두 명시적 선택으로 추가한다. 모르는 키는 공개 키를 신뢰하고, 나머지는
    /// 선택을 기록하는 설치다. 버튼 문구가 선택의 뜻을 말한다.
    #[test]
    fn each_unverified_state_is_added_by_an_explicit_choice() {
        for state in unverified_states() {
            let p = preview(state.clone(), false);
            assert!(state.is_unverified());
            let (action, key) = (add_action(&p), add_button_key(&p, None));
            match state {
                AddTrustState::UnknownKey { .. } => {
                    assert!(matches!(action, PluginsAction::TrustAndInstall { .. }));
                    assert_eq!(key, "plugins.add_trust_key_anyway");
                }
                _ => {
                    assert!(
                        matches!(action, PluginsAction::InstallUnverified { .. }),
                        "{state:?}"
                    );
                    assert_eq!(key, "plugins.add_anyway");
                }
            }
        }
        let trusted = preview(AddTrustState::Trusted, false);
        assert!(matches!(
            add_action(&trusted),
            PluginsAction::Install { .. }
        ));
        assert!(!AddTrustState::Trusted.is_unverified());
    }

    /// 추가 동작은 Verify 때 본 매니페스트의 digest 를 싣는다. 설치는 이 값과 지금 내용을 대조한다.
    #[test]
    fn every_add_action_carries_the_reviewed_manifest_digest() {
        let mut states = unverified_states().to_vec();
        states.push(AddTrustState::Trusted);
        for state in states {
            let digest = match add_action(&preview(state.clone(), false)) {
                PluginsAction::Install {
                    manifest_sha256, ..
                }
                | PluginsAction::InstallUnverified {
                    manifest_sha256, ..
                }
                | PluginsAction::TrustAndInstall {
                    manifest_sha256, ..
                } => manifest_sha256,
                other => panic!("{other:?}"),
            };
            assert_eq!(digest, "digest", "{state:?}");
        }
    }

    fn run_frames(
        ui_state: &mut PluginsUiState,
        height: f32,
        events: &[Vec<egui::Event>],
    ) -> (Vec<PluginsAction>, egui::FullOutput) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(880.0, height));
        let ctx = egui::Context::default();
        let mut actions = Vec::new();
        let mut output = None;
        for frame in events {
            let raw = egui::RawInput {
                screen_rect: Some(screen),
                events: frame.clone(),
                ..Default::default()
            };
            output = Some(ctx.run(raw, |ctx| {
                draw_add_tab(ctx, &PluginsSnapshot::default(), ui_state, &mut actions);
            }));
        }
        (actions, output.expect("drawn"))
    }

    fn key(key: egui::Key) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect()
    }

    /// 미인증 판정에서도 액션 바(권한 수·Cancel·그래도 추가)가 창 안에 보이고, 충분히 높은 창이면
    /// 상자 제목과 원인 문장도 보인다.
    #[test]
    fn action_bar_stays_inside_the_window_below_an_unverified_warning() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
        crate::i18n::init("en");
        for trust_state in unverified_states() {
            let p = preview(trust_state.clone(), false);
            let (kind, _) = trust_kind(&trust_state);
            let wanted = [
                t("plugins.add_grants_none").to_string(),
                t("button.cancel").to_string(),
                t(add_button_key(&p, None)).to_string(),
            ];
            let tall = [
                t("plugins.trust_unverified_title").to_string(),
                t(unverified_cause_key(kind)).to_string(),
            ];
            for height in [300.0, 560.0, 760.0] {
                let mut ui_state = PluginsUiState {
                    add_preview: Some(p.clone()),
                    ..Default::default()
                };
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(880.0, height));
                let (_, output) = run_frames(&mut ui_state, height, &[vec![]]);
                let texts = visible_text_rects(&output, screen);
                // 낮은 창에서는 카드와 신뢰 상자가 스크롤 영역 아래로 밀려도 된다.
                let shown = wanted.iter().chain(tall.iter().filter(|_| height >= 760.0));
                for label in shown {
                    assert!(
                        texts.iter().any(|(text, _)| text == label),
                        "{trust_state:?} at height {height}: {label:?} is not visible inside the window"
                    );
                }
            }
        }
    }

    /// 미인증 판정을 막 보이면 Cancel 이 포커스를 받아 Enter 는 취소한다. Esc 도 취소다.
    /// 어느 쪽도 설치 요청을 보내지 않는다.
    #[test]
    fn enter_and_escape_on_a_fresh_unverified_verdict_cancel() {
        crate::i18n::init("en");
        for keys in [key(egui::Key::Enter), key(egui::Key::Escape)] {
            let mut ui_state = PluginsUiState {
                add_preview: Some(preview(AddTrustState::Unsigned, false)),
                add_path_input: "/tmp/p".into(),
                add_focus_pending: true,
                ..Default::default()
            };
            let (actions, _) = run_frames(&mut ui_state, 760.0, &[vec![], keys, vec![]]);
            assert!(actions.is_empty(), "{actions:?}");
            assert!(ui_state.add_preview.is_none());
            assert!(ui_state.add_path_input.is_empty());
        }
    }

    #[test]
    fn folder_chooser_escape_preserves_an_unverified_preview() {
        crate::i18n::init("en");
        let directory = tempfile::tempdir().unwrap();
        let mut state = PluginsUiState {
            active_tab: super::super::PluginsTab::Add,
            add_preview: Some(preview(AddTrustState::Unsigned, false)),
            add_path_input: "/tmp/p".into(),
            add_focus_pending: true,
            ..Default::default()
        };
        state.folder_chooser.open(
            "plugins.add",
            crate::view::file_chooser::FileChooserMode::Folder,
            directory.path().to_str().unwrap(),
        );
        let ctx = egui::Context::default();
        let mut actions = Vec::new();
        let mut frame = |state: &mut PluginsUiState, events| {
            drop(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(880.0, 760.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    super::super::draw_plugins_panel(
                        ctx,
                        &PluginsSnapshot::default(),
                        state,
                        &mut actions,
                    )
                },
            ));
        };
        frame(&mut state, vec![]);
        frame(&mut state, key(egui::Key::Escape));
        assert_eq!(
            state.add_path_input, "/tmp/p",
            "chooser Esc must preserve the underlying input"
        );
        assert!(state.add_preview.is_some());
        assert!(!state.folder_chooser.is_open());
        frame(&mut state, key(egui::Key::Escape));
        assert!(
            state.add_preview.is_none(),
            "a later Esc keeps the existing unverified-cancel contract"
        );
        assert!(state.add_path_input.is_empty());
        assert!(actions.is_empty());
    }

    /// 추가 요청을 보낸 뒤 결과가 오기 전에는 두 번째 요청을 보내지 않는다. 실패하면 판정이 남고,
    /// 성공하면 입력 화면으로 돌아간다.
    #[test]
    fn an_add_in_flight_sends_one_request_and_a_failure_keeps_the_verdict() {
        let mut ui_state = PluginsUiState {
            add_preview: Some(preview(AddTrustState::Unsigned, false)),
            add_in_flight: true,
            ..Default::default()
        };
        let (actions, _) = run_frames(&mut ui_state, 760.0, &[vec![], key(egui::Key::Escape)]);
        assert!(actions.is_empty());
        assert!(ui_state.add_preview.is_some(), "busy state ignores Esc");
        ui_state.finish_add(false);
        assert!(!ui_state.add_in_flight);
        assert!(ui_state.add_preview.is_some());
        ui_state.add_in_flight = true;
        ui_state.finish_add(true);
        assert!(ui_state.add_preview.is_none());
    }

    /// Add 탭은 설치·부팅과 같은 판정 함수를 쓰므로 debug 에서도 서명 없는 폴더는 미인증이다.
    #[test]
    fn the_add_tab_judges_an_unsigned_folder_as_unverified() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("tasty-plugin.toml"),
            "id = \"com.example.p\"\n",
        )
        .expect("manifest");
        assert_eq!(
            compute_trust_state(dir.path(), "com.example.p"),
            AddTrustState::Unsigned
        );
    }

    /// 매니페스트 읽기 실패와 선언 검사 실패는 서로 다른 제목의 오류 상자로 남는다.
    #[test]
    fn validation_failure_is_told_apart_from_a_read_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let validate = |ui_state: &mut PluginsUiState| {
            ui_state.add_path_input = dir.path().to_string_lossy().into_owned();
            try_validate_path(ui_state, &PluginsSnapshot::default());
        };
        let mut ui_state = PluginsUiState::default();

        // 파일이 없으면 읽기 실패다.
        validate(&mut ui_state);
        let missing = ui_state.add_error.clone().expect("missing manifest");
        assert_eq!(missing.title_key(), "plugins.add_read_error");

        // TOML로 읽을 수 없어도 읽기 실패다.
        std::fs::write(dir.path().join("tasty-plugin.toml"), "id = ").expect("write");
        validate(&mut ui_state);
        let broken = ui_state.add_error.clone().expect("broken toml");
        assert_eq!(broken.title_key(), "plugins.add_read_error");

        // 읽었지만 선언 검사에 실패하면 검증 실패이고, 둘째 줄은 검사 메시지다.
        std::fs::write(
            dir.path().join("tasty-plugin.toml"),
            r#"
                manifest_version = 1
                id = "Not A Valid Id"
                name = "X"
                version = "0.1"
                api_version = "1"
                [entry]
                type = "process"
                command = "x"
            "#,
        )
        .expect("write");
        validate(&mut ui_state);
        let invalid = ui_state.add_error.clone().expect("invalid manifest");
        assert_eq!(invalid.title_key(), "plugins.add_invalid");
        assert!(
            invalid.reason().contains("invalid plugin id"),
            "{invalid:?}"
        );
        assert!(ui_state.add_preview.is_none());
    }
}

/// Picking only supplies the path; existing manifest/trust validation remains the caller's job.
pub(super) fn apply_folder_result(ui_state: &mut PluginsUiState, snapshot: &PluginsSnapshot) {
    if let Some(outcome) = ui_state.folder_chooser.take_outcome("plugins.add") {
        apply_folder_outcome(ui_state, snapshot, outcome);
    }
}

fn apply_folder_outcome(
    ui_state: &mut PluginsUiState,
    snapshot: &PluginsSnapshot,
    outcome: crate::view::file_chooser::FileChooserOutcome,
) {
    if let Some(path) = outcome.into_first_path() {
        ui_state.add_path_input = path.to_string_lossy().into_owned();
        try_validate_path(ui_state, snapshot);
    }
}

#[cfg(test)]
mod folder_tests;
