//! 플러그인 추가 화면 예제. 경로 선택 블록, 안내 상자, 매니페스트 카드, 신뢰 상자, 액션 바를
//! 본체와 같은 공용 view로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    ControlSize, PLUGIN_ADD_INSET, PluginAddBarView, PluginAddPickerView, PluginManifestCardView,
    PluginTrustKind, plugin_add_bar, plugin_add_empty_hint, plugin_add_path_picker,
    plugin_add_read_error, plugin_manifest_card, plugin_trust_box,
};

/// 디자인 `SAMPLE_MANIFEST`. 미신뢰 · 공개키 있음.
const SAMPLE_FINGERPRINT: &str = "9f2c 4ad1 b770 e3a6  ·  ed25519";

/// 16바이트를 넘는 colon-hex fingerprint. 본체가 보이는 SHA-256 전체 표기와 같은 길이다.
const LONG_FINGERPRINT: &str = "1a:2b:3c:4d:5e:6f:70:81:92:a3:b4:c5:d6:e7:f8:09:\
                                 10:21:32:43:54:65:76:87:98:a9:ba:cb:dc:ed:fe:0f";

/// 경로 선택 블록 아래 문단. 본체 `plugins.add_help`의 영어 문구다.
fn help() -> &'static str {
    crate::i18n::t("plugins.add_help")
}

fn sample_strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// 시안 `AddPluginForm` — 경로 선택 블록 아래에 안내 상자 또는 매니페스트 카드와 신뢰 상자,
/// 맨 아래 액션 바. 본체 `draw_add_form`과 같은 공용 view를 쓴다.
pub(super) fn form_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, verified: bool) {
    ui.painter_at(rect)
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let bar_h = ControlSize::Md.height(theme) + theme.spacing_md.value() * 2.0;
    let body = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - bar_h));
    let inner = body.shrink(theme.spacing_md.value());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_max_width(theme.measure_xl.value().min(inner.width()));
    child.spacing_mut().item_spacing.y = theme.spacing_lg.value();

    let mut path = if verified { "~/dev/tasty-logwatch" } else { "" }.to_owned();
    plugin_add_path_picker(
        &mut child,
        theme,
        &PluginAddPickerView {
            label: "Plugin folder",
            placeholder: "~/dev/my-plugin",
            find: "Find folder…",
            verify: "Verify",
            verify_enabled: verified,
            help: help(),
        },
        &mut path,
    );
    if verified {
        child.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = PLUGIN_ADD_INSET.value();
            manifest_card(ui, theme, false);
            trust_box(ui, theme, PluginTrustKind::UnknownKey, SAMPLE_FINGERPRINT);
        });
    } else {
        plugin_add_empty_hint(
            &mut child,
            theme,
            "Choose a folder and press ",
            "Verify",
            " to read its manifest.",
        );
    }

    let bar = egui::Rect::from_min_max(egui::pos2(rect.min.x, body.max.y), rect.max);
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar));
    if verified {
        action_bar(
            &mut bar_ui,
            theme,
            BarSpec {
                focus_cancel: false,
                ..BarSpec::of(PluginTrustKind::UnknownKey)
            },
        );
    } else {
        plugin_add_bar(
            &mut bar_ui,
            theme,
            &PluginAddBarView {
                left: "",
                cancel: "Cancel",
                add: None,
                add_enabled: false,
                danger: false,
                busy: false,
                progress: "",
                focus_cancel: false,
            },
        );
    }
}

/// 시안 `SAMPLE_MANIFEST` 카드. `open_values`면 Homepage 링크와 빈 목록(`None`)을 보인다.
fn manifest_card(ui: &mut egui::Ui, theme: &Theme, open_values: bool) {
    let authors = sample_strings(&["aurelia"]);
    let perms = if open_values {
        Vec::new()
    } else {
        sample_strings(&["fs:read", "fs:watch", "ipc:logwatch.*"])
    };
    let surfaces = if open_values {
        Vec::new()
    } else {
        sample_strings(&["logwatch.viewer"])
    };
    plugin_manifest_card(
        ui,
        theme,
        &PluginManifestCardView {
            name: "logwatch",
            version: "0.3.1",
            id: "com.aurelia.logwatch",
            authors: &authors,
            description: "Tails and highlights structured log files as a dedicated surface — severity filters, a jump-to-error gutter, and live follow on the active workspace.",
            permissions_label: "Permissions",
            permissions: &perms,
            surface_kinds_label: "Surface kinds",
            surface_kinds: &surfaces,
            source_label: "Source",
            source: "~/dev/tasty-logwatch",
            homepage_label: "Homepage",
            homepage: if open_values {
                "https://github.com/aurelia/tasty-logwatch"
            } else {
                ""
            },
            none: "None",
        },
    );
}

/// Verify 전 안내 상자(점선), 매니페스트 읽기 오류 상자, 검증 실패 상자. 본체 열 폭 `width` 안에 셋을 쌓는다.
pub(super) fn hint_slot(ui: &mut egui::Ui, theme: &Theme, width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
        plugin_add_empty_hint(
            ui,
            theme,
            "Choose a folder and press ",
            "Verify",
            " to read its manifest.",
        );
        plugin_add_read_error(
            ui,
            theme,
            crate::i18n::t("plugins.add_read_error"),
            "TOML parse error at line 4, column 9: expected `=`",
        );
        plugin_add_read_error(
            ui,
            theme,
            crate::i18n::t("plugins.add_invalid"),
            "binary \"bin/imgview\" not found in the plugin folder",
        );
    });
}

/// batch 2 회신의 열린 값 — Homepage 링크, 빈 목록 `None`, 긴 fingerprint 의 앞뒤 8바이트.
pub(super) fn open_values(ui: &mut egui::Ui, theme: &Theme, width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = PLUGIN_ADD_INSET.value();
        manifest_card(ui, theme, true);
        trust_box(ui, theme, PluginTrustKind::UnknownKey, LONG_FINGERPRINT);
    });
}

/// 디자인 `TRUST_KIND`의 제목과 본문 문단. 미인증 네 가지는 원인 문장과 공통 위험 문단이고,
/// Add 탭의 모르는 키는 키도 신뢰한다는 문장을 덧붙인다(`with_key_note`).
fn trust_copy(kind: PluginTrustKind, with_key_note: bool) -> (&'static str, Vec<String>) {
    let t = crate::i18n::t;
    match kind {
        PluginTrustKind::Trusted => ("", vec![t("plugins.trust_trusted_body").to_owned()]),
        PluginTrustKind::PermissionsChanged => (
            t("plugins.trust_permissions_changed_title"),
            vec![t("plugins.trust_permissions_changed_body").to_owned()],
        ),
        _ => {
            let cause = match kind {
                PluginTrustKind::UnknownKey => "plugins.trust_cause_unknown_key",
                PluginTrustKind::MissingPubkey => "plugins.trust_cause_missing_pubkey",
                PluginTrustKind::Unsigned => "plugins.trust_cause_unsigned",
                _ => "plugins.trust_cause_signature_error",
            };
            let mut risk = t("plugins.trust_unverified_risk").to_owned();
            if with_key_note && kind == PluginTrustKind::UnknownKey {
                risk.push(' ');
                risk.push_str(t("plugins.trust_key_note"));
            }
            (
                t("plugins.trust_unverified_title"),
                vec![t(cause).to_owned(), risk],
            )
        }
    }
}

/// 신뢰 상자. `fingerprint`가 비면 fingerprint 줄을 그리지 않는다(확인 필요 탭의 상자).
pub(super) fn unverified_box(
    ui: &mut egui::Ui,
    theme: &Theme,
    kind: PluginTrustKind,
    with_key_note: bool,
    fingerprint: &str,
) {
    let (title, body) = trust_copy(kind, with_key_note);
    let body: Vec<&str> = body.iter().map(String::as_str).collect();
    plugin_trust_box(ui, theme, kind, title, &body, |ui| {
        if !fingerprint.is_empty() {
            super::attention::fingerprint_line(ui, theme, fingerprint);
        }
    });
}

fn trust_box(ui: &mut egui::Ui, theme: &Theme, kind: PluginTrustKind, fingerprint: &str) {
    unverified_box(ui, theme, kind, true, fingerprint);
}

/// 디자인 `grantsLabel`.
fn grants(perms: usize) -> String {
    match perms {
        0 => "No permissions".to_owned(),
        n => crate::i18n::t_count("plugins.add_grants", n as u64, &[&n.to_string()]),
    }
}

/// 액션 바 한 줄의 상태 — 디자인 `Bar`의 판정·진행·포커스.
#[derive(Clone, Copy)]
struct BarSpec {
    kind: PluginTrustKind,
    blocked: bool,
    perms: usize,
    busy: bool,
    focus_cancel: bool,
}

impl BarSpec {
    const fn of(kind: PluginTrustKind) -> Self {
        Self {
            kind,
            blocked: false,
            perms: 3,
            busy: false,
            focus_cancel: false,
        }
    }
}

/// 프리뷰 하단 액션 바 — 본체 `draw_add_footer`의 버튼 줄.
/// 막혔으면 추가 버튼을 disabled로 두고 이유를 왼쪽에 적는다. 미인증이면 추가 버튼이 danger 다.
fn action_bar(ui: &mut egui::Ui, theme: &Theme, spec: BarSpec) {
    let t = crate::i18n::t;
    let grants = grants(spec.perms);
    let label = if spec.blocked {
        t("plugins.add_button")
    } else {
        match spec.kind {
            PluginTrustKind::Trusted => t("plugins.add_button"),
            PluginTrustKind::PermissionsChanged => t("plugins.add_trust_and_add"),
            PluginTrustKind::UnknownKey => t("plugins.add_trust_key_anyway"),
            _ => t("plugins.add_anyway"),
        }
    };
    plugin_add_bar(
        ui,
        theme,
        &PluginAddBarView {
            left: if spec.blocked {
                t("plugins.add_blocked_installed")
            } else {
                &grants
            },
            cancel: "Cancel",
            add: Some(label),
            add_enabled: !spec.blocked,
            danger: !spec.blocked && spec.kind.is_unverified(),
            busy: spec.busy,
            progress: t("plugins.add_adding"),
            focus_cancel: spec.focus_cancel,
        },
    );
}

/// 디자인 `Bar`·`AddBarG` 줄. 신뢰·권한 변경·미인증 네 가지(모르는 키는 Cancel 포커스),
/// 추가 중, 이미 설치됨, 권한 1개·0개.
pub(super) fn add_bars(ui: &mut egui::Ui, theme: &Theme, width: f32) {
    use PluginTrustKind as K;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for spec in [
            BarSpec::of(K::Trusted),
            BarSpec::of(K::PermissionsChanged),
            BarSpec {
                focus_cancel: true,
                ..BarSpec::of(K::UnknownKey)
            },
            BarSpec::of(K::MissingPubkey),
            BarSpec::of(K::Unsigned),
            BarSpec::of(K::SignatureError),
            BarSpec {
                busy: true,
                ..BarSpec::of(K::Unsigned)
            },
            BarSpec {
                blocked: true,
                ..BarSpec::of(K::Trusted)
            },
            BarSpec {
                perms: 1,
                ..BarSpec::of(K::Trusted)
            },
            BarSpec {
                perms: 0,
                ..BarSpec::of(K::Trusted)
            },
        ] {
            egui::Frame::new()
                .fill(theme.bg_panel().to_egui())
                .show(ui, |ui| {
                    ui.set_width(width);
                    action_bar(ui, theme, spec);
                });
        }
    });
}

/// 디자인 `TrustBox2` 여섯 가지. 서명이 없거나 깨진 상자에는 fingerprint 줄이 없다.
pub(super) fn trust_boxes(ui: &mut egui::Ui, theme: &Theme, width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for (label, ground) in [
            ("Trusted · mantle (host Add tab)", theme.bg_sidebar()),
            ("Trusted · base (design board)", theme.bg_panel()),
        ] {
            crate::catalog::spec::cluster(ui, theme, label, |ui| {
                egui::Frame::new().fill(ground.to_egui()).show(ui, |ui| {
                    ui.set_width(width);
                    trust_box(ui, theme, PluginTrustKind::Trusted, "");
                });
            });
        }
        for kind in [
            PluginTrustKind::PermissionsChanged,
            PluginTrustKind::UnknownKey,
            PluginTrustKind::MissingPubkey,
            PluginTrustKind::Unsigned,
            PluginTrustKind::SignatureError,
        ] {
            trust_box(ui, theme, kind, SAMPLE_FINGERPRINT);
        }
    });
}
