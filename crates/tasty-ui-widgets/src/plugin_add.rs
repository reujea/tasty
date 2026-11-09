//! Plugins 창 Add plugin 화면의 경로 선택, 매니페스트 카드, 신뢰 판정 상자, 액션 바와
//! fingerprint 줄·서명 무효 설명. 본체와 갤러리가 함께 호출한다.
//!
//! 이 view는 매니페스트를 읽거나 서명을 검증하지 않는다. 문구와 판정 결과는 호출부가 넘기고
//! 눌린 동작만 돌려준다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use std::borrow::Cow;

use crate::button::{Button, ButtonVariant};
use crate::chip::TagVariant;
use crate::control::ControlSize;
use crate::icon_button::{IconButton, IconButtonVariant};
use crate::input::Input;
use crate::plugin_avatar::{PluginAvatarSize, plugin_avatar};
use crate::tokens::STRUCT_GAP_3;
use crate::tooltip::{Tooltip, tooltip_hover_delay_elapsed};

/// 신뢰 상자와 액션 바의 가로 여백, 카드와 신뢰 상자 사이 간격.
/// 디자인 `plugins_window.jsx`의 `--tasty-size-14`·`gap: 14`이며 역할 토큰이 없다.
pub const PLUGIN_ADD_INSET: LogicalPx = LogicalPx(14.0);

/// 신뢰 판정 여섯 가지. 상자 톤은 추가가 무엇을 하는지를 나타낸다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginTrustKind {
    /// 신뢰 목록의 키로 서명됐다. 그대로 추가한다.
    Trusted,
    /// 신뢰한 게시자지만 권한이 늘었다. 추가하면 새 권한 묶음을 신뢰한다.
    PermissionsChanged,
    /// 미인증 — 신뢰 목록에 없는 키다. 추가하면 공개 키 파일의 키도 신뢰한다.
    UnknownKey,
    /// 미인증 — 서명은 있지만 공개 키 파일이 없거나 읽을 수 없다.
    MissingPubkey,
    /// 미인증 — 서명이 없다. fingerprint 줄이 없다.
    Unsigned,
    /// 미인증 — 이 파일들로 서명을 검증하지 못했다. fingerprint 줄이 없다.
    SignatureError,
}

impl PluginTrustKind {
    /// 미인증 네 가지. 같은 danger 상자·위험 문단을 쓰고, 사용자가 명시적으로 골라야 추가한다.
    pub fn is_unverified(self) -> bool {
        !matches!(
            self,
            PluginTrustKind::Trusted | PluginTrustKind::PermissionsChanged
        )
    }

    fn tone(self, theme: &Theme) -> HexColor {
        match self {
            PluginTrustKind::Trusted => theme.accent_success(),
            PluginTrustKind::PermissionsChanged => theme.accent_warning(),
            _ => theme.accent_danger(),
        }
    }

    fn glyph_color(self, theme: &Theme) -> HexColor {
        match self {
            PluginTrustKind::Trusted => theme.plugin_trust_glyph_success(),
            PluginTrustKind::PermissionsChanged => theme.plugin_trust_glyph_warning(),
            _ => theme.plugin_trust_glyph_danger(),
        }
    }

    fn glyph(self) -> tasty_icons::Icon {
        match self {
            PluginTrustKind::Trusted => tasty_icons::SHIELD_CHECK,
            PluginTrustKind::PermissionsChanged => tasty_icons::ALERT_TRIANGLE,
            _ => tasty_icons::ALERT_CIRCLE,
        }
    }

    /// 상자 아래에 fingerprint 줄을 두는지. 서명이 없거나 깨지면 식별할 서명이 없다.
    pub fn shows_fingerprint(self) -> bool {
        matches!(
            self,
            PluginTrustKind::PermissionsChanged
                | PluginTrustKind::UnknownKey
                | PluginTrustKind::MissingPubkey
        )
    }
}

/// 신뢰 판정 상자. 톤 색의 tint 채움과 테두리를 쓴다.
/// `Trusted`는 글리프와 `body` 첫 문단 한 줄이고, 나머지는 제목 줄(톤은 글리프만 진다), 본문
/// 문단들, fingerprint 줄 순이다. 미인증은 원인 문장과 위험 문단 두 문단이다.
/// `fingerprint`는 [`PluginTrustKind::shows_fingerprint`]가 참일 때만 불린다.
pub fn plugin_trust_box(
    ui: &mut egui::Ui,
    theme: &Theme,
    kind: PluginTrustKind,
    title: &str,
    body: &[&str],
    fingerprint: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let tone = kind.tone(theme).to_egui();
    let glyph = theme.icon_glyph_size_md.value();
    let gap = theme.spacing_sm.value();
    let term_sm = theme.font_size_term_sm.value();
    egui::Frame::new()
        .fill(tone.gamma_multiply(theme.tint_fill_alpha()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            tone.gamma_multiply(theme.tint_border_alpha()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            PLUGIN_ADD_INSET.value() as i8,
            theme.spacing_md.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            if kind == PluginTrustKind::Trusted {
                ui.horizontal(|ui| {
                    glyph_cell(ui, kind, glyph, kind.glyph_color(theme).to_egui());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(body.first().copied().unwrap_or_default())
                                .size(term_sm)
                                .color(theme.plugin_trust_body_trusted_fg().to_egui()),
                        )
                        .wrap(),
                    );
                });
                return;
            }
            // 600 굵기는 굵은 UI 글꼴이 없어 크기로 근사한다.
            ui.horizontal(|ui| {
                glyph_cell(ui, kind, glyph, kind.glyph_color(theme).to_egui());
                ui.label(
                    egui::RichText::new(title)
                        .size(theme.font_size_body.value())
                        .color(theme.plugin_trust_title_fg().to_egui()),
                );
            });
            for paragraph in body {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(*paragraph)
                            .size(term_sm)
                            .line_height(Some(term_sm * theme.line_height_ui))
                            .color(theme.plugin_trust_body_fg().to_egui()),
                    )
                    .wrap(),
                );
            }
            if kind.shows_fingerprint() {
                fingerprint(ui);
            }
        })
        .response
}

/// 예약 id 사유의 배너. 서명 오류와 형상이 같은 danger 상자(alertCircle, 제목 한 줄, 본문 한 문단)이며
/// fingerprint 줄이 없다. 신뢰 판정이 아니어서 판정 종류를 호출자에게 드러내지 않는다.
pub fn plugin_reserved_id_banner(ui: &mut egui::Ui, theme: &Theme, title: &str, body: &str) {
    plugin_trust_box(
        ui,
        theme,
        PluginTrustKind::SignatureError,
        title,
        &[body],
        |_| {},
    );
}

fn glyph_cell(ui: &mut egui::Ui, kind: PluginTrustKind, size: f32, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    kind.glyph().image(size, color).paint_at(ui, rect);
}

/// 매니페스트 카드의 값과 머리글.
pub struct PluginManifestCardView<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub id: &'a str,
    pub authors: &'a [String],
    pub description: &'a str,
    pub permissions_label: &'a str,
    pub permissions: &'a [String],
    pub surface_kinds_label: &'a str,
    pub surface_kinds: &'a [String],
    pub source_label: &'a str,
    pub source: &'a str,
    pub homepage_label: &'a str,
    /// 비어 있으면 Homepage 줄을 그리지 않는다.
    pub homepage: &'a str,
    /// 권한·surface 종류가 없을 때의 문구.
    pub none: &'a str,
}

/// 매니페스트 카드에서 눌린 동작.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PluginManifestCardOutput {
    /// Homepage 링크를 눌렀다. 호출부가 기본 브라우저로 연다.
    pub open_homepage: bool,
}

/// 매니페스트 카드. identity 줄(아바타 lg · 이름 + 버전 Tag · id · 작성자), 설명,
/// Permissions·Surface kinds Tag 목록, Source, Homepage를 `space-md` 간격으로 쌓는다.
pub fn plugin_manifest_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginManifestCardView<'_>,
) -> PluginManifestCardOutput {
    let mut output = PluginManifestCardOutput::default();
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, theme.spacing_md.value());
            identity(ui, theme, view);
            if !view.description.is_empty() {
                let body = theme.font_size_body.value();
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(view.description)
                            .size(body)
                            .line_height(Some(body * theme.line_height_ui))
                            .color(theme.text_secondary_raised().to_egui()),
                    )
                    .wrap(),
                );
            }
            tag_list(
                ui,
                theme,
                view.permissions_label,
                view.permissions,
                view.none,
            );
            tag_list(
                ui,
                theme,
                view.surface_kinds_label,
                view.surface_kinds,
                view.none,
            );
            mono_field(ui, theme, view.source_label, |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(view.source)
                            .monospace()
                            .size(theme.font_size_term_sm.value())
                            .color(theme.text_secondary_raised().to_egui()),
                    )
                    .truncate(),
                );
            });
            // 웹 주소만 링크로 연다. 다른 scheme 은 같은 자리에 누를 수 없는 평문으로 보인다.
            if !view.homepage.is_empty() {
                mono_field(ui, theme, view.homepage_label, |ui| {
                    if crate::plugin_detail::is_web_homepage(view.homepage) {
                        output.open_homepage = homepage_link(ui, theme, view.homepage).clicked();
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(view.homepage)
                                    .monospace()
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_secondary_raised().to_egui()),
                            )
                            .truncate(),
                        );
                    }
                });
            }
        });
    output
}

/// Homepage 링크. text-secondary 글자에 밑줄을 늘 긋고, hover 면 text-primary, 키보드 포커스면
/// focus ring 을 두른다. 줄이 넘치면 끝을 말줄임한다.
fn homepage_link(ui: &mut egui::Ui, theme: &Theme, url: &str) -> egui::Response {
    let font = egui::FontId::monospace(theme.font_size_caption.value());
    let max_width = ui.available_width();
    let ctx = ui.ctx().clone();
    let layout = |color: egui::Color32| {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            url,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color,
                underline: egui::Stroke::new(theme.border_width.value(), color),
                ..Default::default()
            },
        );
        job.wrap.max_width = max_width;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        ctx.fonts(|f| f.layout_job(job))
    };
    let probe = layout(egui::Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(probe.size(), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if resp.hovered() {
        theme.text_on_active()
    } else {
        theme.text_secondary_raised()
    };
    ui.painter()
        .galley(rect.min, layout(color.to_egui()), color.to_egui());
    if resp.has_focus() {
        ui.painter().rect_stroke(
            rect,
            theme.corner_radius_sm.value(),
            egui::Stroke::new(theme.focus_ring_width.value(), theme.border_focus()),
            egui::StrokeKind::Outside,
        );
    }
    resp
}

/// 아바타 lg 오른쪽에 이름 + 버전 Tag, 그 아래 `id · 첫 작성자 +N`.
fn identity(ui: &mut egui::Ui, theme: &Theme, view: &PluginManifestCardView<'_>) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
        plugin_avatar(ui, theme, view.name, PluginAvatarSize::Detail);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                // 600 굵기는 크기와 색으로 근사한다.
                ui.label(
                    egui::RichText::new(view.name)
                        .size(theme.font_size_max.value())
                        .color(theme.text_primary().to_egui()),
                );
                crate::tag_mono(
                    ui,
                    theme,
                    &format!("v{}", view.version),
                    TagVariant::Default,
                    false,
                );
            });
            ui.add_space(STRUCT_GAP_3.value());
            let resp = ui.add(
                egui::Label::new(
                    egui::RichText::new(id_line(view.id, view.authors))
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted_raised().to_egui()),
                )
                .truncate()
                .sense(egui::Sense::hover()),
            );
            if view.authors.len() > 1
                && tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered())
            {
                Tooltip::new(&view.authors.join(", "))
                    .id_source(resp.id)
                    .show(ui, theme, resp.rect);
            }
        });
    });
}

/// `id · 첫 작성자`에 작성자가 더 있으면 ` +N`을 붙인다. 작성자가 없으면 id만.
fn id_line(id: &str, authors: &[String]) -> String {
    match authors {
        [] => id.to_owned(),
        [first] => format!("{id} · {first}"),
        [first, rest @ ..] => format!("{id} · {first} +{}", rest.len()),
    }
}

/// mono 대문자 머리글. 디자인 `Mono`(micro, text-muted, caps, `letter-spacing-caps`).
pub fn mono_header(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let size = theme.font_size_micro;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(size.value()),
            color: theme.text_muted_raised().to_egui(),
            extra_letter_spacing: theme.letter_spacing_caps(size).value(),
            ..Default::default()
        },
    );
    ui.add(egui::Label::new(job).selectable(false));
}

/// 머리글과 값 한 줄을 `space-xs`로 묶는다.
fn mono_field(ui: &mut egui::Ui, theme: &Theme, label: &str, value: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        mono_header(ui, theme, label);
        value(ui);
    });
}

/// 머리글과 Tag 목록을 `space-sm`으로 묶는다. 목록이 비면 text-muted `none` 문구.
fn tag_list(ui: &mut egui::Ui, theme: &Theme, label: &str, items: &[String], none: &str) {
    let gap = theme.spacing_sm.value();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = gap;
        mono_header(ui, theme, label);
        if items.is_empty() {
            ui.label(
                egui::RichText::new(none)
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted_raised().to_egui()),
            );
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            for item in items {
                crate::tag_mono(ui, theme, item, TagVariant::Default, false);
            }
        });
    });
}

/// 액션 바 문구와 상태.
pub struct PluginAddBarView<'a> {
    /// 왼쪽 문구. 추가할 수 없으면 이유, 아니면 부여할 권한 수. 매니페스트가 없으면 빈 문자열.
    pub left: &'a str,
    pub cancel: &'a str,
    /// 추가 버튼 문구. 매니페스트를 확인하기 전에는 `None`이다. 추가 중에도 바뀌지 않는다.
    pub add: Option<&'a str>,
    pub add_enabled: bool,
    /// 미인증 판정. 추가 버튼을 danger 로 그린다.
    pub danger: bool,
    /// 추가 처리 중. 두 버튼을 끄고 버튼 왼쪽에 Spinner 와 `progress` 를 둔다.
    pub busy: bool,
    /// 추가 중에 버튼 옆에 적는 진행 문구.
    pub progress: &'a str,
    /// 이 프레임에 Cancel 로 포커스를 옮긴다. 미인증 판정을 처음 보일 때 쓴다.
    pub focus_cancel: bool,
}

/// 액션 바에서 눌린 버튼.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PluginAddBarClicks {
    pub add: bool,
    pub cancel: bool,
}

/// 위 구분선 아래에 왼쪽 문구, 오른쪽 Cancel(ghost) + 추가(primary, 미인증이면 danger)를 둔다.
/// 추가할 수 없으면 추가 버튼을 숨기지 않고 disabled로 둔다. 매니페스트를 확인하기 전에는
/// Cancel 만 둔다. 기본 버튼은 없다 — Enter 는 포커스가 있는 버튼만 누른다.
pub fn plugin_add_bar(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginAddBarView<'_>,
) -> PluginAddBarClicks {
    let mut clicks = PluginAddBarClicks::default();
    let response = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(
            PLUGIN_ADD_INSET.value() as i8,
            theme.spacing_md.value() as i8,
        ))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ControlSize::Md.height(theme)),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    if !view.left.is_empty() {
                        ui.label(
                            egui::RichText::new(view.left)
                                .size(theme.font_size_caption.value())
                                .color(theme.text_muted().to_egui()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(add) = view.add {
                            let variant = if view.danger {
                                ButtonVariant::Danger
                            } else {
                                ButtonVariant::Primary
                            };
                            clicks.add = Button::new(add)
                                .variant(variant)
                                .enabled(view.add_enabled && !view.busy)
                                .show(ui, theme)
                                .clicked();
                        }
                        let cancel = Button::new(view.cancel)
                            .variant(ButtonVariant::Ghost)
                            .enabled(!view.busy)
                            .show(ui, theme);
                        if view.focus_cancel && !view.busy {
                            cancel.request_focus();
                        }
                        clicks.cancel = cancel.clicked();
                        if view.busy {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let size = theme.icon_glyph_size_sm.value();
                                    let word =
                                        crate::busy_caption_width(ui, theme, view.progress, size);
                                    ui.add_space((ui.available_width() - word).max(0.0));
                                    crate::busy_caption(ui, theme, view.progress, size);
                                },
                            );
                        }
                    });
                },
            );
        })
        .response;
    let rect = response.rect;
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    clicks
}

/// fingerprint 값에서 앞뒤로 남기는 바이트 수.
const FINGERPRINT_EDGE_BYTES: usize = 8;

/// colon-hex fingerprint 가 16바이트를 넘으면 앞 8바이트 + ` … ` + 뒤 8바이트로 줄인다.
/// 그보다 짧거나 콜론으로 나뉘지 않은 표기는 그대로 둔다.
pub fn short_fingerprint(value: &str) -> Cow<'_, str> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() <= FINGERPRINT_EDGE_BYTES * 2 {
        return Cow::Borrowed(value);
    }
    Cow::Owned(format!(
        "{} … {}",
        parts[..FINGERPRINT_EDGE_BYTES].join(":"),
        parts[parts.len() - FINGERPRINT_EDGE_BYTES..].join(":")
    ))
}

/// fingerprint 줄의 문구.
pub struct PluginFingerprintLineView<'a> {
    pub label: &'a str,
    /// 전체 값. 화면에는 [`short_fingerprint`]로 줄여 보이고 툴팁과 복사는 전체 값이다.
    pub value: &'a str,
    pub copy_tooltip: &'a str,
}

/// fingerprint 라벨, 값, 복사 IconButton 한 줄. 복사 버튼을 누르면 전체 값을 클립보드에 넣고
/// true 를 돌려준다. 값이 없으면 호출하지 않으므로 복사 버튼에는 disabled 상태가 없다.
pub fn plugin_fingerprint_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginFingerprintLineView<'_>,
) -> bool {
    let mut copied = false;
    // 라벨이 버튼보다 먼저 배치되므로 줄 높이를 버튼 높이로 먼저 잡아야 세로 가운데가 맞는다.
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ControlSize::Sm.height(theme)),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            let caption = theme.font_size_caption.value();
            ui.label(
                egui::RichText::new(view.label)
                    .monospace()
                    .size(caption)
                    .color(theme.plugin_trust_body_fg().to_egui()),
            );
            let shown = short_fingerprint(view.value);
            let resp = ui.add(
                egui::Label::new(
                    egui::RichText::new(shown.as_ref())
                        .monospace()
                        .size(caption)
                        .color(theme.plugin_trust_body_fg().to_egui()),
                )
                .sense(egui::Sense::hover()),
            );
            if matches!(shown, Cow::Owned(_))
                && tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered())
            {
                Tooltip::new(view.value)
                    .id_source(resp.id)
                    .show(ui, theme, resp.rect);
            }
            let copy = IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, theme, &|ui, rect, c| {
                    tasty_icons::COPY.image(rect.width(), c).paint_at(ui, rect);
                })
                .on_hover_text(view.copy_tooltip);
            if copy.clicked() {
                ui.ctx().copy_text(view.value.to_owned());
                copied = true;
            }
        },
    );
    copied
}

/// Attention › Signature invalid 절의 머리글 아래 내용. `note` 는 term-sm text-muted 문단,
/// `cause` 는 그 아래 mono caption text-muted 줄이다.
pub fn plugin_signature_invalid_detail(
    ui: &mut egui::Ui,
    theme: &Theme,
    note: &str,
    cause: Option<&str>,
) {
    let term_sm = theme.font_size_term_sm.value();
    let width = theme.measure_lg.value().min(ui.available_width());
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.add(
            egui::Label::new(
                egui::RichText::new(note)
                    .size(term_sm)
                    .line_height(Some(term_sm * theme.line_height_ui))
                    .color(theme.text_muted().to_egui()),
            )
            .wrap(),
        );
    });
    if let Some(cause) = cause {
        ui.add(
            egui::Label::new(
                egui::RichText::new(cause)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            )
            .wrap(),
        );
    }
}

/// 경로 선택 블록의 문구.
pub struct PluginAddPickerView<'a> {
    /// mono 머리글. 디자인 `Plugin folder`.
    pub label: &'a str,
    pub placeholder: &'a str,
    /// 폴더 선택 버튼 라벨.
    pub find: &'a str,
    pub verify: &'a str,
    /// 경로가 비었고 확인한 매니페스트도 없으면 false.
    pub verify_enabled: bool,
    /// 아래 설명 문단. 백틱으로 감싼 부분은 mono text-secondary 로 그린다.
    pub help: &'a str,
}

/// 경로 선택 블록에서 일어난 일.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PluginAddPickerOutput {
    /// 경로를 고쳤다. 호출부는 확인한 매니페스트를 버린다.
    pub changed: bool,
    /// 폴더 선택 버튼을 눌렀다.
    pub find: bool,
    /// Verify 를 눌렀거나 입력에서 Enter 를 눌렀다.
    pub verify: bool,
}

/// 경로 선택 블록. mono 머리글, mono 입력(폴더 아이콘) + 폴더 선택(secondary) + Verify(primary),
/// 설명 문단을 `space-sm` 간격으로 쌓는다. 폭은 `measure-xl` 이 상한이다.
pub fn plugin_add_path_picker(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginAddPickerView<'_>,
    path: &mut String,
) -> PluginAddPickerOutput {
    let mut out = PluginAddPickerOutput::default();
    let width = theme.measure_xl.value().min(ui.available_width());
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            mono_header(ui, theme, view.label);
            ui.allocate_ui_with_layout(
                egui::vec2(width, ControlSize::Md.height(theme)),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    out.verify = Button::new(view.verify)
                        .variant(ButtonVariant::Primary)
                        .enabled(view.verify_enabled)
                        .show(ui, theme)
                        .clicked();
                    out.find = Button::new(view.find)
                        .variant(ButtonVariant::Secondary)
                        .leading_icon(&|ui, rect, c| {
                            tasty_icons::FOLDER
                                .image(rect.height(), c)
                                .paint_at(ui, rect)
                        })
                        .show(ui, theme)
                        .clicked();
                    let resp = Input::new()
                        .mono(true)
                        .placeholder(view.placeholder)
                        .icon(&|ui, rect, c| {
                            tasty_icons::FOLDER
                                .image(rect.height(), c)
                                .paint_at(ui, rect)
                        })
                        .show(ui, theme, path);
                    out.changed = resp.changed();
                    if resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && view.verify_enabled
                    {
                        out.verify = true;
                    }
                },
            );
            help_paragraph(ui, theme, view.help);
        },
    );
    out
}

/// 백틱 사이를 mono text-secondary 로, 나머지를 term-sm text-muted 로 그린 문단.
fn help_paragraph(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let term_sm = theme.font_size_term_sm.value();
    let mut job = egui::text::LayoutJob::default();
    for (i, part) in text.split('`').enumerate() {
        let code = i % 2 == 1;
        let (font_id, color) = if code {
            (
                egui::FontId::monospace(term_sm),
                theme.text_secondary().to_egui(),
            )
        } else {
            (
                egui::FontId::proportional(term_sm),
                theme.text_muted().to_egui(),
            )
        };
        job.append(
            part,
            0.0,
            egui::TextFormat {
                font_id,
                color,
                line_height: Some(term_sm * theme.line_height_ui),
                ..Default::default()
            },
        );
    }
    job.wrap.max_width = ui.available_width();
    ui.add(egui::Label::new(job).wrap());
}

/// 매니페스트를 확인하기 전의 안내 상자. 폴더 아이콘과 `before` · `emphasis` · `after` 한 줄을
/// border-default 1px 점선 상자에 담는다. `emphasis` 는 text-secondary 다.
/// 점선은 곧은 변에만 두고 모서리는 실선 호다([`crate::paint_dashed_outline`]).
pub fn plugin_add_empty_hint(
    ui: &mut egui::Ui,
    theme: &Theme,
    before: &str,
    emphasis: &str,
    after: &str,
) {
    let term_sm = theme.font_size_term_sm.value();
    let muted = theme.text_muted().to_egui();
    let frame = hint_slot_frame(ui, theme, egui::Color32::TRANSPARENT, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            let glyph = theme.icon_glyph_size_md.value();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(glyph, glyph), egui::Sense::hover());
            tasty_icons::FOLDER.image(glyph, muted).paint_at(ui, rect);
            let mut job = egui::text::LayoutJob::default();
            for (text, color) in [
                (before, muted),
                (emphasis, theme.text_secondary().to_egui()),
                (after, muted),
            ] {
                job.append(
                    text,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(term_sm),
                        color,
                        ..Default::default()
                    },
                );
            }
            ui.add(egui::Label::new(job).wrap());
        });
    });
    crate::paint_dashed_outline(
        ui.painter(),
        theme,
        frame.rect,
        theme.corner_radius.value(),
        theme.border_default().to_egui(),
    );
}

/// Verify가 `tasty-plugin.toml`을 읽지 못했을 때 안내 상자 자리에 두는 상자.
/// 안내 상자와 같은 크기에 accent-danger 실선 테두리, alertTriangle과 `title`,
/// 그 아래 읽기 오류 원문 `reason` 한 줄(mono caption, 번역하지 않음)을 둔다. 채움과 동작은 없다.
pub fn plugin_add_read_error(ui: &mut egui::Ui, theme: &Theme, title: &str, reason: &str) {
    let danger = theme.accent_danger().to_egui();
    let ink = theme.accent_danger_raised().to_egui();
    hint_slot_frame(ui, theme, danger, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            let glyph = theme.icon_glyph_size_md.value();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(glyph, glyph), egui::Sense::hover());
            tasty_icons::ALERT_TRIANGLE
                .image(glyph, ink)
                .paint_at(ui, rect);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.label_detail_gap.value();
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(title)
                            .size(theme.font_size_body.value())
                            .color(ink),
                    )
                    .wrap(),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(reason)
                            .monospace()
                            .size(theme.font_size_caption.value())
                            .color(theme.text_muted().to_egui()),
                    )
                    .wrap_mode(egui::TextWrapMode::Wrap),
                );
            });
        });
    });
}

/// 안내 상자와 읽기 오류 상자가 함께 쓰는 틀. 바깥 폭은 `measure_xl`(남은 폭이 좁으면 남은 폭),
/// 안쪽 여백은 가로 space-lg · 세로 14, 반경 radius, 테두리는 `edge` 색의 1px 실선이다.
/// 점선 상자는 투명 테두리로 같은 크기를 잡고 호출부가 점선을 그린다.
fn hint_slot_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    edge: egui::Color32,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let width = theme.measure_xl.value().min(ui.available_width());
    egui::Frame::new()
        .stroke(egui::Stroke::new(theme.border_width.value(), edge))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_lg.value() as i8,
            PLUGIN_ADD_INSET.value() as i8,
        ))
        .show(ui, |ui| {
            // 바깥 폭이 `width`가 되도록 안쪽 여백과 테두리를 뺀다.
            ui.set_width(width - (theme.spacing_lg.value() + theme.border_width.value()) * 2.0);
            content(ui);
        })
        .response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_line_names_the_first_author_and_counts_the_rest() {
        let authors = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(id_line("com.a.b", &[]), "com.a.b");
        assert_eq!(id_line("com.a.b", &authors(&["ann"])), "com.a.b · ann");
        assert_eq!(
            id_line("com.a.b", &authors(&["ann", "bo", "cy"])),
            "com.a.b · ann +2"
        );
    }

    #[test]
    fn long_colon_hex_fingerprints_keep_eight_bytes_at_each_end() {
        let sha256 = (0..32)
            .map(|i| format!("{i:02x}"))
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(
            short_fingerprint(&sha256),
            "00:01:02:03:04:05:06:07 … 18:19:1a:1b:1c:1d:1e:1f"
        );
        let sixteen = (0..16)
            .map(|i| format!("{i:02x}"))
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(short_fingerprint(&sixteen), sixteen);
        let spaced = "9f2c 4ad1 b770 e3a6  ·  ed25519";
        assert_eq!(short_fingerprint(spaced), spaced);
    }

    /// 매니페스트 카드의 Homepage 도 웹 주소만 누를 수 있는 링크다.
    #[test]
    fn the_manifest_card_links_only_web_homepages() {
        let card = |homepage: &'static str| {
            move |ui: &mut egui::Ui, theme: &Theme| {
                plugin_manifest_card(
                    ui,
                    theme,
                    &PluginManifestCardView {
                        name: "Git helper",
                        version: "1.0.0",
                        id: "com.a.b",
                        authors: &[],
                        description: "",
                        permissions_label: "Permissions",
                        permissions: &[],
                        surface_kinds_label: "Surface kinds",
                        surface_kinds: &[],
                        source_label: "Source",
                        source: "/tmp/p",
                        homepage_label: "Homepage",
                        homepage,
                        none: "None",
                    },
                )
                .open_homepage
            }
        };
        let click = crate::plugin_detail::tests::click_text;
        assert_eq!(
            click("https://example.com", card("https://example.com")),
            (true, true)
        );
        assert_eq!(
            click("file:///etc/passwd", card("file:///etc/passwd")),
            (false, false)
        );
    }

    #[test]
    fn only_signature_errors_and_trusted_boxes_have_no_fingerprint() {
        assert!(!PluginTrustKind::Trusted.shows_fingerprint());
        assert!(PluginTrustKind::UnknownKey.shows_fingerprint());
        assert!(PluginTrustKind::PermissionsChanged.shows_fingerprint());
        assert!(PluginTrustKind::MissingPubkey.shows_fingerprint());
        assert!(!PluginTrustKind::SignatureError.shows_fingerprint());
    }
}
