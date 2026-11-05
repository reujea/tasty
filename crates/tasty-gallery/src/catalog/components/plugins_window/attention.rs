//! 서명·권한·실행 오류로 확인이 필요한 플러그인 예제.
//! 본체는 선택한 하나를 표시하지만 갤러리는 네 사유를 나란히 보여준다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::{PLUGIN_LIST_ROW_HEIGHT, STRUCT_GAP_2};
use tasty_ui_widgets::{
    PluginAttentionBarAction, PluginAttentionBarView, PluginAvatarSize, PluginFingerprintLineView,
    PluginIdentityView, PluginMetaView, PluginTrustKind, is_web_homepage, margin_all,
    paint_plugin_avatar, plugin_attention_bar, plugin_detail_bar_height, plugin_detail_desc_hidden,
    plugin_detail_description, plugin_detail_identity, plugin_fingerprint_line, plugin_mono_header,
    plugin_signature_invalid_detail,
};

/// 본체 ATTN_PRIMITIVE_12와 같은 12px 글꼴. 대응 semantic 토큰이 없다.
const ATTN_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    UnknownKey,
    SignatureInvalid,
    PermissionsChanged,
    HealthError,
}

impl Kind {
    /// 본체 `AttentionKind::is_danger` — 서명 계열만 danger, 나머지는 warning.
    fn is_danger(self) -> bool {
        matches!(self, Self::UnknownKey | Self::SignatureInvalid)
    }

    fn label(self) -> &'static str {
        match self {
            Self::UnknownKey => crate::i18n::t("plugins.attn_unknown_key_label"),
            Self::SignatureInvalid => "Signature invalid",
            Self::PermissionsChanged => "Permissions changed",
            Self::HealthError => "Runtime error",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Self::UnknownKey => {
                "Signed by a key that isn't in your trust store — registration rejected."
            }
            Self::SignatureInvalid => {
                "Signature missing or failed verification — registration rejected."
            }
            Self::PermissionsChanged => {
                "Manifest permissions changed since you trusted it — re-approval required."
            }
            Self::HealthError => crate::i18n::t("plugins.attn_health_blurb"),
        }
    }

    /// 액션 바 우측 버튼 — 본체 `draw_action_bar` 의 사유별 분기.
    /// 서명 사유는 danger `Register anyway` 다. 누르기 전에는 등록하지 않는다.
    fn action(self) -> Option<PluginAttentionBarAction<'static>> {
        match self {
            Self::PermissionsChanged => Some(PluginAttentionBarAction::Reapprove(crate::i18n::t(
                "plugins.attn_reapprove",
            ))),
            Self::HealthError => Some(PluginAttentionBarAction::Configure(crate::i18n::t(
                "plugins.configure",
            ))),
            Self::UnknownKey | Self::SignatureInvalid => {
                Some(PluginAttentionBarAction::RegisterAnyway(crate::i18n::t(
                    "plugins.attn_register_anyway",
                )))
            }
        }
    }

    /// 서명 사유의 미인증 상자 종류. 본체는 거부 기록의 원인 이름으로 고른다.
    fn trust_kind(self) -> Option<PluginTrustKind> {
        match self {
            Self::UnknownKey => Some(PluginTrustKind::UnknownKey),
            Self::SignatureInvalid => Some(PluginTrustKind::SignatureError),
            Self::PermissionsChanged | Self::HealthError => None,
        }
    }

    fn status(self) -> &'static str {
        if self.is_danger() {
            "Not registered"
        } else {
            "Needs review"
        }
    }
}

pub(super) struct Entry {
    pub name: &'static str,
    pub version: &'static str,
    pub id: &'static str,
    pub authors: &'static str,
    /// 매니페스트 설명. 본체처럼 서명이 깨진 사유는 비운다.
    pub description: &'static str,
    /// 매니페스트 homepage. 본체처럼 서명이 깨진 사유는 비운다.
    pub homepage: &'static str,
    pub builtin: bool,
    pub kind: Kind,
}

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        name: "Port scanner",
        version: "0.2.0",
        id: "com.example.port-scanner",
        authors: "example",
        description: "Lists listening ports per workspace.",
        homepage: "https://example.com/port-scanner",
        builtin: false,
        kind: Kind::UnknownKey,
    },
    Entry {
        name: "Log tailer",
        version: "0.1.4",
        id: "com.example.log-tailer",
        authors: "example",
        description: "",
        homepage: "",
        builtin: false,
        kind: Kind::SignatureInvalid,
    },
    Entry {
        name: "Git viewer",
        version: "0.3.1",
        id: "com.tasty.git-viewer",
        authors: "tasty",
        description: "Read-only status, log and diff of the current repository.",
        homepage: "https://github.com/zilhak/tasty",
        builtin: true,
        kind: Kind::PermissionsChanged,
    },
    Entry {
        name: "Markdown",
        version: "0.9.0",
        id: "com.tasty.markdown",
        authors: "tasty",
        description: "Renders Markdown files as a surface.",
        homepage: "https://github.com/zilhak/tasty",
        builtin: true,
        kind: Kind::HealthError,
    },
];

/// 상세에 펼쳐 보이는 기본 항목 — 본체는 선택 행 하나를 그린다.
pub(super) const SELECTED: usize = 2;
/// 서명이 깨진 항목. 상세가 설명과 homepage 대신 숨김 안내를 보인다.
pub(super) const SIGNATURE_INVALID: usize = 1;

fn sev_color(theme: &Theme, kind: Kind) -> egui::Color32 {
    if kind.is_danger() {
        theme.accent_danger().to_egui()
    } else {
        theme.accent_warning().to_egui()
    }
}

/// 좌측 목록 — 본체 `SidePanel::left("plugins_attention_list")`. 행 높이·패딩은
/// Installed 목록과 같은 토큰 조립이고, 두 번째 줄이 사유 라벨(severity 색)이다.
pub(super) fn list_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, selected: usize) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let row_h = PLUGIN_LIST_ROW_HEIGHT.value();
    let pad = egui::vec2(theme.spacing_sm.value(), theme.spacing_sm.value());
    let avatar = PluginAvatarSize::Row.side().value();
    let mut y = rect.min.y + theme.spacing_sm.value();

    for (i, entry) in ENTRIES.iter().enumerate() {
        let r =
            egui::Rect::from_min_size(egui::pos2(rect.min.x, y), egui::vec2(rect.width(), row_h));
        if i == selected {
            p.rect(
                r,
                theme.corner_radius.value(),
                theme.surface_active().to_egui(),
                egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
                egui::StrokeKind::Inside,
            );
        }
        paint_plugin_avatar(
            &p,
            theme,
            egui::pos2(r.min.x + pad.x + avatar * 0.5, r.center().y),
            entry.name,
            PluginAvatarSize::Row,
        );

        let name_pos = r.min + pad + egui::vec2(avatar + theme.spacing_sm.value(), 0.0);
        p.text(
            name_pos,
            egui::Align2::LEFT_TOP,
            entry.name,
            egui::FontId::proportional(theme.font_size_body.value()),
            if i == selected {
                theme.text_on_active()
            } else {
                theme.text_primary()
            }
            .to_egui(),
        );
        p.text(
            name_pos + egui::vec2(0.0, theme.spacing_lg.value() + STRUCT_GAP_2.value()),
            egui::Align2::LEFT_TOP,
            entry.kind.label(),
            egui::FontId::proportional(theme.font_size_micro.value()),
            if i == selected {
                theme.text_on_active().to_egui()
            } else {
                sev_color(theme, entry.kind)
            },
        );
        p.circle_filled(
            egui::pos2(r.max.x - theme.spacing_md.value(), r.center().y),
            theme.status_dot_size.value() * 0.5,
            sev_color(theme, entry.kind),
        );
        y += row_h + STRUCT_GAP_2.value();
    }
}

/// 사유 배너 — 본체 `draw_detail` 의 severity 프레임. 서명 사유는 Add 탭과 같은 미인증 상자이고
/// fingerprint 는 아래 Signature 절이 보인다.
fn banner(ui: &mut egui::Ui, theme: &Theme, kind: Kind) {
    if let Some(trust) = kind.trust_kind() {
        super::add::unverified_box(ui, theme, trust, false, "");
        return;
    }
    let color = sev_color(theme, kind);
    egui::Frame::new()
        .fill(color.gamma_multiply(theme.tint_fill_alpha()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            color.gamma_multiply(theme.tint_border_alpha()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(margin_all(theme.spacing_md))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            ui.label(
                egui::RichText::new(kind.label())
                    .strong()
                    .size(theme.font_size_body.value())
                    .color(color),
            );
            ui.label(
                egui::RichText::new(kind.blurb())
                    .size(ATTN_PRIMITIVE_12.value())
                    .color(theme.text_secondary().to_egui()),
            );
        });
}

/// 사유별 추가 정보 — 본체 `draw_reason_detail` 의 세 분기.
fn reason_detail(ui: &mut egui::Ui, theme: &Theme, kind: Kind) {
    // 본체와 같은 디자인 `Mono` 머리글.
    let mono_header = |ui: &mut egui::Ui, text: &str| plugin_mono_header(ui, theme, text);
    match kind {
        Kind::PermissionsChanged => {
            mono_header(ui, "Permission changes");
            for (sign, token, note, color) in [
                (
                    "+",
                    "fs:write",
                    "newly requested",
                    theme.accent_success().to_egui(),
                ),
                (
                    "−",
                    "clipboard",
                    "no longer used",
                    theme.text_muted().to_egui(),
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(sign).monospace().strong().color(color));
                    ui.label(
                        egui::RichText::new(token)
                            .monospace()
                            .size(ATTN_PRIMITIVE_12.value()),
                    );
                    ui.label(
                        egui::RichText::new(note)
                            .size(theme.font_size_caption.value())
                            .color(theme.text_muted().to_egui()),
                    );
                });
            }
        }
        Kind::UnknownKey | Kind::SignatureInvalid => {
            mono_header(ui, "Signature");
            // 본체처럼 서명 무효에는 fingerprint 줄 대신 고정 설명과 실패 원인을 둔다.
            if kind == Kind::SignatureInvalid {
                plugin_signature_invalid_detail(
                    ui,
                    theme,
                    crate::i18n::t("plugins.attn_sig_invalid_note"),
                    Some("tasty-plugin.toml.sig sidecar missing"),
                );
            } else {
                fingerprint_line(ui, theme, "SHA256:9f2c…a17e");
            }
        }
        Kind::HealthError => {
            mono_header(ui, "Log");
            egui::Frame::new()
                .fill(theme.bg_panel().to_egui())
                .stroke(egui::Stroke::new(
                    theme.border_width.value(),
                    theme.separator.to_egui_premultiplied(),
                ))
                .corner_radius(theme.corner_radius.value())
                .inner_margin(margin_all(theme.spacing_sm))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("exited with status 101 (panicked at doc.rs:88)")
                            .monospace()
                            .size(ATTN_PRIMITIVE_12.value())
                            .color(theme.accent_danger_raised().to_egui()),
                    );
                });
        }
    }
}

/// 복사 버튼 툴팁. 본체와 같은 번역 키를 `lang/en.toml`에서 읽는다.
/// 갤러리는 설정을 읽지 않으므로 처음 쓸 때 영어 번역표로 한 번 초기화한다.
fn copy_fingerprint_tooltip() -> &'static str {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let report = tasty_i18n::init("en");
        if report.fell_back() {
            tracing::warn!("gallery i18n init fell back: {report:?}");
        }
    });
    tasty_i18n::t("plugins.attn_copy_fingerprint")
}

/// fingerprint 라벨·값·복사 IconButton 한 줄 — 본체 `fingerprint_line`과 같은 공용 view.
pub(super) fn fingerprint_line(ui: &mut egui::Ui, theme: &Theme, value: &str) {
    plugin_fingerprint_line(
        ui,
        theme,
        &PluginFingerprintLineView {
            label: "fingerprint",
            value,
            copy_tooltip: copy_fingerprint_tooltip(),
        },
    );
}

/// 상태 점 + 상태 텍스트 + 우측 조치 버튼 — Installed 와 같은 바 틀의 공용 위젯.
fn action_bar(ui: &mut egui::Ui, theme: &Theme, kind: Kind) {
    plugin_attention_bar(
        ui,
        theme,
        &PluginAttentionBarView {
            status: kind.status(),
            color: sev_color(theme, kind),
            action: kind.action(),
        },
    );
}

/// 우측 상세 — identity → 배너 → 사유 detail, 열 바닥에 열 폭 전체의 액션 바.
pub(super) fn detail_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, selected: usize) {
    ui.painter_at(rect)
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let entry = &ENTRIES[selected];
    let inner = rect.shrink(theme.spacing_md.value());
    let split = rect.max.y - plugin_detail_bar_height(theme);
    let body_rect = egui::Rect::from_min_max(inner.min, egui::pos2(inner.max.x, split));
    let bar_rect = egui::Rect::from_min_max(egui::pos2(rect.min.x, split), rect.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
    // 본체는 본문을 스크롤 안에 두므로 바 뒤로 넘친 글이 보이지 않는다. 예제도 본문을 바 위에서 자른다.
    child.set_clip_rect(body_rect.intersect(ui.clip_rect()));
    // 디자인은 정체 블록 · 설명 · 배너 · 사유 detail 을 `space-lg` 간격으로 쌓는다. 블록 안은 `space-sm`.
    child.spacing_mut().item_spacing.y = theme.spacing_lg.value();

    plugin_detail_identity(
        &mut child,
        theme,
        &PluginIdentityView {
            name: entry.name,
            version: entry.version,
            builtin_tag: entry
                .builtin
                .then(|| crate::i18n::t("plugins.builtin_badge")),
            meta: PluginMetaView {
                authors: entry.authors,
                id: entry.id,
                // 본체처럼 웹 주소일 때만 링크로 싣는다. 서명이 깨진 사유는 비어 있다.
                homepage: if is_web_homepage(entry.homepage) {
                    entry.homepage
                } else {
                    ""
                },
            },
        },
    );
    if entry.kind == Kind::SignatureInvalid {
        plugin_detail_desc_hidden(
            &mut child,
            theme,
            crate::i18n::t("plugins.attn_desc_hidden"),
        );
    } else if !entry.description.is_empty() {
        plugin_detail_description(&mut child, theme, entry.description);
    }
    banner(&mut child, theme, entry.kind);
    child.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        reason_detail(ui, theme, entry.kind);
    });

    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
    action_bar(&mut bar, theme, entry.kind);
}

/// 상세 영역의 빈 상태. 목록 배경과 너비는 유지한다.
pub(super) fn empty_detail_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    ui.painter_at(rect)
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let inner = rect.shrink(theme.spacing_md.value());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.add_space(theme.spacing_xl.value() * 2.0);
    child.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(crate::i18n::t("plugins.attn_empty_title"))
                .size(theme.font_size_body.value())
                .color(theme.text_secondary().to_egui()),
        );
        ui.label(
            egui::RichText::new(crate::i18n::t("plugins.attn_empty_body"))
                .size(ATTN_PRIMITIVE_12.value())
                .color(theme.text_muted().to_egui()),
        );
    });
}

/// 빈 상태의 목록 패널 — 행이 없다. 배경과 폭은 그대로다.
pub(super) fn empty_list_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    ui.painter_at(rect)
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
}

/// 비교 카드마다 세로 배치와 위쪽 정렬을 적용해 내용이 가로로 넘치지 않게 한다.
pub(super) fn reason_cards(ui: &mut egui::Ui, theme: &Theme) {
    let card_w = theme.measure_sm.value() * 0.5;
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
        |ui| cards_row(ui, theme, card_w),
    );
}

fn cards_row(ui: &mut egui::Ui, theme: &Theme, card_w: f32) {
    for kind in [
        Kind::UnknownKey,
        Kind::SignatureInvalid,
        Kind::PermissionsChanged,
        Kind::HealthError,
    ] {
        ui.allocate_ui_with_layout(
            egui::vec2(card_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui: &mut egui::Ui| {
                ui.set_max_width(card_w);
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                banner(ui, theme, kind);
                reason_detail(ui, theme, kind);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 상태 이름이 말하는 사유와 펼치는 항목의 사유가 같고, 서명이 깨진 항목은 본체처럼 매니페스트
    /// 글을 싣지 않는다.
    #[test]
    fn the_signature_invalid_state_opens_a_signature_invalid_entry() {
        let entry = &ENTRIES[SIGNATURE_INVALID];
        assert!(entry.kind == Kind::SignatureInvalid);
        assert!(entry.description.is_empty() && entry.homepage.is_empty());
        assert!(ENTRIES[SELECTED].kind != Kind::SignatureInvalid);
    }
}
