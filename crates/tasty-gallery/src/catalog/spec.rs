//! 갤러리의 구역·예제·설명·토큰 표를 배치하는 공용 헬퍼.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// 예제 영역의 레이아웃 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StageVariant {
    /// 가로로 배치하고 폭을 넘으면 다음 줄로 보낸다.
    #[default]
    Wrap,
    /// 안쪽 여백 없이 표·탭바 등을 보여준다.
    Tight,
    /// 세로 적층.
    Column,
    /// 가로 중앙 정렬.
    Center,
    /// 큰 단독 예제를 세로로 배치한다.
    Solo,
}

/// "Tokens used" 칩 한 개 — 색 스와치 + 토큰명 + 용도.
/// 색이 없는 토큰(치수·불투명도·폰트 등)은 시안 `Meta`처럼 스와치를 그리지 않는다.
#[derive(Clone, Copy)]
pub struct TokenChip {
    pub tok: &'static str,
    pub use_: &'static str,
    pub color: Option<egui::Color32>,
}

impl TokenChip {
    pub fn new(tok: &'static str, use_: &'static str, color: egui::Color32) -> Self {
        Self {
            tok,
            use_,
            color: Some(color),
        }
    }

    /// 색 스와치 없이 토큰명과 용도만 보이는 칩.
    pub fn without_color(tok: &'static str, use_: &'static str) -> Self {
        Self {
            tok,
            use_,
            color: None,
        }
    }
}

#[inline]
fn col(h: impl Into<egui::Color32>) -> egui::Color32 {
    h.into()
}

/// 구역 제목과 아래 구분선을 그린다.
pub fn section(ui: &mut egui::Ui, theme: &Theme, title: &str) {
    ui.add_space(theme.spacing_xl.value() + theme.spacing_lg.value());
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(theme.font_size_term_sm.value())
            .color(col(theme.text_muted())),
    );
    ui.add_space(theme.spacing_sm.value());
    hline(ui, theme, col(theme.separator.to_egui_premultiplied()));
    ui.add_space(theme.spacing_sm.value());
}

/// 예제 제목과 사용 상황 설명을 그린다.
pub fn spec(ui: &mut egui::Ui, theme: &Theme, title: &str, when: Option<&str>) {
    ui.add_space(theme.spacing_xl.value());
    ui.label(
        egui::RichText::new(title)
            .size(theme.font_size_term_lg.value())
            .strong()
            .color(col(theme.text_primary())),
    );
    if let Some(w) = when {
        ui.add_space(theme.spacing_xs.value());
        ui.label(
            egui::RichText::new(w)
                .size(theme.font_size_body.value())
                .color(col(theme.text_secondary())),
        );
    }
    ui.add_space(theme.spacing_md.value());
}

/// 예제 영역의 테두리·배경과 종류별 여백·레이아웃을 적용한다.
pub fn stage(
    ui: &mut egui::Ui,
    theme: &Theme,
    variant: StageVariant,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let pad = match variant {
        StageVariant::Tight => 0.0,
        _ => theme.spacing_xl.value(),
    };
    egui::Frame::new()
        .fill(col(theme.bg_panel()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            col(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(pad as i8))
        .show(ui, |ui| match variant {
            StageVariant::Column => {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
                    add_contents(ui);
                });
            }
            StageVariant::Center => {
                ui.vertical_centered(add_contents);
            }
            // 큰 단독 예제와 여백 없는 예제는 세로 배치를 사용한다.
            StageVariant::Solo | StageVariant::Tight => {
                ui.vertical(add_contents);
            }
            StageVariant::Wrap => {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing =
                        egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
                    add_contents(ui);
                });
            }
        });
}

/// 라벨과 예제들을 한 묶음으로 배치한다.
pub fn cluster(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let body = |ui: &mut egui::Ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.label(
            egui::RichText::new(label.to_uppercase())
                .size(theme.font_size_micro.value())
                .color(col(theme.text_muted())),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
            add_contents(ui);
        });
    };
    wrap_item(ui, body);
}

/// 줄바꿈 줄(stage Wrap · `horizontal_wrapped`)에 놓는 묶음 하나. 감싸는 줄은 크기를 모르는
/// 자식 ui 를 다음 줄로 보내지 못하므로, 직전 프레임에 잰 크기로 자리를 요청해 시안
/// `flex-wrap: wrap` 처럼 칸을 넘기면 줄을 바꾼다. 묶음 안은 세로 배치다.
pub fn wrap_item(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let id = ui.next_auto_id().with("wrap_item");
    let prev: Option<egui::Vec2> = ui.data(|d| d.get_temp(id));
    let rect = match prev {
        Some(size) => {
            ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), add_contents)
                .response
                .rect
        }
        None => ui.vertical(add_contents).response.rect,
    };
    ui.data_mut(|d| d.insert_temp(id, rect.size()));
}

/// 왼쪽에는 치수 설명을, 오른쪽에는 사용한 토큰을 표시한다.
/// `tokens` 가 비면 1컬럼(Layout spec)만 그린다.
pub fn meta(ui: &mut egui::Ui, theme: &Theme, specs: &[(&str, &str)], tokens: &[TokenChip]) {
    // 시안 `.meta`는 창 폭 900 이하에서 두 열을 한 열로 쌓는다.
    let stacked = ui.ctx().screen_rect().width() <= META_STACK_MAX_W.value();
    body_column(ui, |ui| {
        egui::Frame::new()
            .fill(col(theme.bg_panel()))
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                col(theme.separator.to_egui_premultiplied()),
            ))
            .corner_radius(theme.corner_radius_sm.value())
            .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
            .show(ui, |ui| {
                if tokens.is_empty() || stacked {
                    // 시안 `.meta`는 한 열일 때도 본문 폭을 채운다.
                    ui.set_min_width(ui.available_width());
                    meta_specs(ui, theme, specs);
                    if !tokens.is_empty() {
                        ui.add_space(theme.spacing_md.value());
                        meta_tokens(ui, theme, tokens);
                    }
                } else {
                    ui.columns(2, |cols| {
                        meta_specs(&mut cols[0], theme, specs);
                        meta_tokens(&mut cols[1], theme, tokens);
                    });
                }
            });
    });
}

/// 시안 `.meta`가 한 열로 쌓이는 창 폭 상한(`@media (max-width: 900px)`).
const META_STACK_MAX_W: LogicalPx = LogicalPx(900.0);

/// 시안 `.dl`처럼 키 열은 가장 긴 키의 폭, 값 열은 남은 폭이다. 긴 값은 값 열 안에서 줄바꿈한다.
fn meta_specs(ui: &mut egui::Ui, theme: &Theme, specs: &[(&str, &str)]) {
    meta_head(ui, theme, "Layout spec");
    let font = egui::FontId::proportional(theme.font_size_term_sm.value());
    let key_w = ui.fonts(|f| {
        specs
            .iter()
            .map(|(k, _)| {
                f.layout_no_wrap((*k).to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                    .size()
                    .x
            })
            .fold(0.0, f32::max)
    });
    let gap = theme.spacing_md.value();
    for (k, v) in specs {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                egui::vec2(key_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // 자식 영역은 사용한 폭만 할당하므로 키 열 폭을 직접 채운다.
                    ui.set_min_width(key_w);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(*k)
                                .size(theme.font_size_term_sm.value())
                                .color(col(theme.text_muted())),
                        )
                        .extend(),
                    );
                },
            );
            let value_w = ui.available_width().max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(value_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(*v)
                                .size(theme.font_size_term_sm.value())
                                .color(col(theme.text_primary())),
                        )
                        .wrap(),
                    );
                },
            );
        });
    }
}

/// 시안 `.chips`처럼 칩을 가로로 놓고 폭을 넘으면 다음 줄로 보낸다.
/// 칩은 `.chip`처럼 surface-raised 배경과 border-default 1px 테두리를 두르고,
/// 스와치는 border-strong 1px 테두리를 둘러 스와치 색이 패널 배경과 같아도 구분된다.
/// 칩 사이·칩 안 간격과 세로 여백은 space-xs, 가로 여백은 space-sm, 글꼴은 caption,
/// 스와치는 icon-size-xs 다(시안 `.chips`·`.chip`).
fn meta_tokens(ui: &mut egui::Ui, theme: &Theme, tokens: &[TokenChip]) {
    meta_head(ui, theme, "Tokens used");
    let font = egui::FontId::monospace(theme.font_size_caption.value());
    let gap = theme.spacing_xs.value();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        for t in tokens {
            token_chip(ui, theme, t, &font);
        }
    });
}

/// 줄바꿈 배치가 칩 전체를 다음 줄로 넘길 수 있도록 크기를 먼저 계산해 할당한 뒤 그린다.
fn token_chip(ui: &mut egui::Ui, theme: &Theme, t: &TokenChip, font: &egui::FontId) {
    let pad = egui::vec2(theme.spacing_sm.value(), theme.spacing_xs.value());
    let gap = theme.spacing_xs.value();
    let sw = theme.icon_glyph_size_xs.value();
    let tok =
        ui.fonts(|f| f.layout_no_wrap(t.tok.to_owned(), font.clone(), col(theme.text_primary())));
    let sw_w = if t.color.is_some() { sw + gap } else { 0.0 };
    // 시안 `.chip`은 inline-flex라 줄 폭보다 길면 용도 글자가 칩 안에서 줄바꿈된다.
    let use_wrap = (ui.max_rect().width() - pad.x * 2.0 - sw_w - tok.size().x - gap).max(sw);
    let use_ = (!t.use_.is_empty()).then(|| {
        ui.fonts(|f| {
            f.layout(
                format!("— {}", t.use_),
                font.clone(),
                col(theme.text_muted_raised()),
                use_wrap,
            )
        })
    });
    let use_w = use_.as_ref().map_or(0.0, |g| gap + g.size().x);
    let text_h = tok.size().y.max(use_.as_ref().map_or(0.0, |g| g.size().y));
    let size = egui::vec2(
        pad.x * 2.0 + sw_w + tok.size().x + use_w,
        pad.y * 2.0 + text_h.max(sw),
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let radius = theme.corner_radius_sm.value();
    let border = theme.border_width.value();
    painter.rect_filled(rect, radius, col(theme.surface_raised()));
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(border, col(theme.border_default())),
        egui::StrokeKind::Inside,
    );
    let mut x = rect.min.x + pad.x;
    // 용도 글자가 여러 줄이면 스와치와 토큰 이름은 첫 줄에 맞춘다.
    let cy = rect.min.y + pad.y + text_h.min(tok.size().y.max(sw)) * 0.5;
    if let Some(color) = t.color {
        let r = egui::Rect::from_center_size(egui::pos2(x + sw * 0.5, cy), egui::vec2(sw, sw));
        painter.rect_filled(r, radius, color);
        painter.rect_stroke(
            r,
            radius,
            egui::Stroke::new(border, col(theme.border_strong())),
            egui::StrokeKind::Inside,
        );
        x += sw_w;
    }
    let tok_w = tok.size().x;
    let tok_y = cy - tok.size().y * 0.5;
    painter.galley(egui::pos2(x, tok_y), tok, col(theme.text_primary()));
    if let Some(g) = use_ {
        let y = rect.min.y + pad.y;
        painter.galley(
            egui::pos2(x + tok_w + gap, y),
            g,
            col(theme.text_muted_raised()),
        );
    }
}

fn meta_head(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(theme.font_size_micro.value())
            .color(col(theme.text_muted())),
    );
    ui.add_space(theme.spacing_sm.value());
}

/// `note`/`meta`/`do_`·`dont` 와 host_shell 이 공유하는 본문 컬럼 폭 temp-data 키.
pub(crate) fn body_column_width_id() -> egui::Id {
    egui::Id::new("g_body_column_width")
}

/// 예제가 가용 폭을 늘려도 설명이 창 밖으로 잘리지 않도록 본문 컬럼 폭에 맞춘다.
/// 저장된 컬럼 폭이 없으면 현재 가용 폭을 사용한다.
pub(crate) fn body_column<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let wrap_w = ui
        .data(|d| d.get_temp::<f32>(body_column_width_id()))
        .filter(|w| *w > 0.0)
        .unwrap_or_else(|| ui.available_width());
    ui.allocate_ui_with_layout(
        egui::vec2(wrap_w, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| add(ui),
    )
    .inner
}

/// 보조 설명을 작은 글씨로 표시한다.
pub fn note(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add_space(theme.spacing_md.value());
    body_column(ui, |ui| {
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(theme.font_size_term_sm.value())
                    .color(col(theme.text_muted())),
            )
            .wrap(),
        );
    });
}

/// 권장 사항을 success 색으로 표시한다.
pub fn do_(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    accent_bar(ui, theme, text, col(theme.accent_success()));
}

/// 피할 사항을 danger 색으로 표시한다.
pub fn dont(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    accent_bar(ui, theme, text, col(theme.accent_danger()));
}

fn accent_bar(ui: &mut egui::Ui, theme: &Theme, text: &str, accent: egui::Color32) {
    ui.add_space(theme.spacing_sm.value());
    body_column(ui, |ui| {
        // 배경은 tinted 상자 관용구의 채움 계수(`--tasty-tint-fill-alpha`)로 만든다.
        let tint = accent.gamma_multiply(theme.tint_fill_alpha());
        let resp = egui::Frame::new()
            .fill(tint)
            .corner_radius(theme.corner_radius_sm.value())
            .inner_margin(egui::Margin {
                left: theme.spacing_md.value() as i8,
                right: theme.spacing_md.value() as i8,
                top: theme.spacing_sm.value() as i8,
                bottom: theme.spacing_sm.value() as i8,
            })
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(theme.font_size_term_sm.value())
                        .color(col(theme.text_secondary())),
                );
            });
        let r = resp.response.rect;
        let bar =
            egui::Rect::from_min_size(r.min, egui::vec2(theme.tint_edge_width.value(), r.height()));
        ui.painter().rect_filled(bar, 0.0, accent);
    });
}

/// 현재 ui 폭 전체에 1px separator 라인을 그린다 (세로 공간도 예약).
fn hline(ui: &mut egui::Ui, theme: &Theme, color: egui::Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(theme.border_width.value(), color),
    );
}
