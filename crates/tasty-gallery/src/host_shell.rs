//! 상단 도구 모음·왼쪽 탐색·오른쪽 예제 본문으로 갤러리를 구성한다.
//! Catalog는 페이지를, On this page는 현재 페이지의 구역을 선택한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::{self, Category, Page};

/// 디자인의 10px 탐색 여백. egui::Margin에 맞춰 i8로 둔다.
const NAV_PAD_X: i8 = 10;

const NAV_WIDTH: LogicalPx = LogicalPx(232.0);
const HEADER_HEIGHT: LogicalPx = LogicalPx(52.0);
/// 본문을 읽기 좋은 폭으로 제한하는 디자인 값.
const PAGE_MAX_WIDTH: LogicalPx = LogicalPx(1080.0);

/// 갤러리에 노출하는 빌트인 테마 식별자.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    Mocha,
    Latte,
}

impl ThemeId {
    /// `mocha`·`latte`(대소문자 무시). 다른 값은 None이다.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "mocha" => Some(ThemeId::Mocha),
            "latte" => Some(ThemeId::Latte),
            _ => None,
        }
    }

    /// 해당 id 의 색상 세트와 테마가 정한 밝기로 Theme 을 만든다.
    /// 밝기를 따로 고르는 토글은 없으므로 직전 테마의 밝기를 이어받지 않는다.
    fn build(self) -> Theme {
        match self {
            ThemeId::Mocha => tasty_themes::mocha_fallback(),
            ThemeId::Latte => latte_theme(),
        }
    }
}

/// Mocha 기본값에 내장 Latte 설정을 덮어쓴다. 고정 Latte 예제에서도 사용한다.
pub(crate) fn latte_theme() -> Theme {
    use tasty_type_appearance::theme::Theme as ThemeStruct;
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT)
        .expect("builtin latte.toml ships valid");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    ThemeStruct::with_colors(colors, is_light.unwrap_or(true))
}

/// 본체 AppearanceSettings가 제공하는 UI 배율을 그대로 사용한다.
fn ui_scale_stops() -> Vec<(String, f32)> {
    tasty_settings::UI_SCALE_CHOICES
        .iter()
        .map(|key| {
            let factor = tasty_settings::AppearanceSettings::ui_scale_factor_for(key);
            // 0.85 / 1.0 / 1.2처럼 표시하되 불필요한 끝자리 0만 지운다.
            let padded = format!("{factor:.2}");
            let label = padded.strip_suffix('0').unwrap_or(&padded).to_string();
            (label, factor)
        })
        .collect()
}

/// 갤러리 전역 상태.
pub struct GalleryState {
    /// 현재 적용 중인 테마. 본체 글로벌과 격리된 갤러리 전용 인스턴스.
    pub theme: Theme,
    /// 선택된 테마 식별자 (Theme 구조체엔 id 가 없어 별도 보관).
    pub theme_id: ThemeId,
    /// 갤러리 페이지 목록.
    pub pages: Vec<Page>,
    /// 현재 활성 페이지 index (`pages` 기준).
    pub active_page: usize,
    /// egui 전역 배율. Theme 치수에 다시 곱하지 않는다.
    pub ui_scale: f32,
    /// Specs 선택 상태. 격자 오버레이는 아직 구현하지 않는다.
    pub specs_on: bool,
    /// 다음 frame 에서 visuals/zoom 을 ctx 에 재적용할지.
    pub needs_reapply: bool,
    /// 배치 캡처의 본문 스크롤 위치. 일반 실행은 None이다.
    pub shot_scroll: Option<f32>,
    /// 좌상단 brand 로고 텍스처 (앱 아이콘 PNG 디코드 결과, 1회 캐시).
    brand_logo: Option<egui::TextureHandle>,
}

impl GalleryState {
    pub fn new() -> Self {
        let theme_id = ThemeId::Mocha;
        Self {
            theme: theme_id.build(),
            theme_id,
            pages: catalog::pages(),
            active_page: 0,
            ui_scale: 1.0,
            specs_on: false,
            needs_reapply: true,
            shot_scroll: None,
            brand_logo: None,
        }
    }

    /// 툴바 밖 진입점(시작 env 등)에서 테마를 고른다.
    pub fn select_theme(&mut self, id: ThemeId) {
        self.switch_theme(id);
    }

    /// 테마를 바꾸고 다음 frame 에 egui 에 다시 적용하게 한다. 같은 테마면 아무것도 하지 않는다.
    fn switch_theme(&mut self, id: ThemeId) {
        if id == self.theme_id {
            return;
        }
        self.theme_id = id;
        self.theme = id.build();
        self.needs_reapply = true;
    }

    /// 페이지 index 를 범위 내로 고정해 활성화 (스크린샷 배치 등 외부 진입점용).
    pub fn select_page(&mut self, idx: usize) {
        self.active_page = idx.min(self.pages.len().saturating_sub(1));
    }
}

impl Default for GalleryState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw(ctx: &egui::Context, state: &mut GalleryState) {
    if state.needs_reapply {
        tasty_egui_theme::apply_theme_to_egui(&state.theme, ctx);
        ctx.set_zoom_factor(state.ui_scale);
        state.needs_reapply = false;
    }

    let sidebar_bg = egui::Color32::from(state.theme.bg_sidebar());
    let main_bg = egui::Color32::from(state.theme.bg_app());
    egui::TopBottomPanel::top("g_header")
        .exact_height(HEADER_HEIGHT.value())
        .frame(egui::Frame::new().fill(sidebar_bg))
        .show(ctx, |ui| header_ui(ui, state));
    egui::SidePanel::left("g_nav")
        .exact_width(NAV_WIDTH.value())
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(sidebar_bg)
                .inner_margin(egui::Margin {
                    left: NAV_PAD_X,
                    right: NAV_PAD_X,
                    top: state.theme.spacing_md.value() as i8,
                    bottom: state.theme.spacing_xl.value() as i8,
                }),
        )
        .show(ctx, |ui| nav_ui(ui, state));
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(main_bg)
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ctx, |ui| main_ui(ui, state));
}

/// 상단 헤더 — brand 블록(좌 232) + top bar(crumb + Specs/Theme/Scale 세그).
fn header_ui(ui: &mut egui::Ui, state: &mut GalleryState) {
    // closure 안에서 state를 갱신하므로 Theme의 필요한 값을 먼저 복사한다.
    let (primary, muted, melon, border, surf_raised, surf_active, separator) = {
        let t = &state.theme;
        (
            egui::Color32::from(t.text_primary()),
            egui::Color32::from(t.text_muted()),
            egui::Color32::from(t.brand_melon_flesh()),
            egui::Color32::from(t.border_default()),
            egui::Color32::from(t.surface_raised()),
            egui::Color32::from(t.surface_active()),
            t.separator.to_egui_premultiplied(),
        )
    };
    let seg = SegStyle {
        border,
        surf_raised,
        surf_active,
        on_active: egui::Color32::from(state.theme.text_on_active()),
        muted,
        radius: state.theme.corner_radius.value(),
        border_w: state.theme.border_width.value(),
        height: state.theme.item_height_interactive.value(),
        font: state.theme.font_size_term_sm.value(),
    };
    let logo = state.theme.sidebar_logo_size.value();
    let f_word = state.theme.sidebar_wordmark_font_size.value();
    let f_micro = state.theme.font_size_micro.value();
    let f_body = state.theme.font_size_body.value();
    let pad_lg = state.theme.spacing_lg.value();
    let pad_sm = state.theme.spacing_sm.value();

    let page_label = state
        .pages
        .get(state.active_page)
        .map(|p| p.category.label())
        .unwrap_or("");
    let theme_id = state.theme_id;
    let ui_scale = state.ui_scale;
    let specs_on = state.specs_on;

    // closure 밖에서 state에 반영할 선택 결과.
    let mut act_theme: Option<ThemeId> = None;
    let mut act_specs: Option<bool> = None;
    let mut act_scale: Option<f32> = None;

    let logo_tex = brand_logo_texture(ui.ctx(), &mut state.brand_logo);

    ui.horizontal_centered(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(NAV_WIDTH.value(), HEADER_HEIGHT.value()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(pad_lg);
                let (r, _) = ui.allocate_exact_size(egui::vec2(logo, logo), egui::Sense::hover());
                // 디자인의 앱 아이콘 크기와 모양을 유지한다.
                egui::Image::from_texture(&logo_tex)
                    .fit_to_exact_size(egui::vec2(logo, logo))
                    .paint_at(ui, r);
                ui.add_space(pad_sm);
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(
                    egui::RichText::new("tasty")
                        .size(f_word)
                        .strong()
                        .color(primary),
                );
                ui.label(egui::RichText::new(".").size(f_word).strong().color(melon));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(pad_lg);
                    ui.label(egui::RichText::new("gallery").size(f_micro).color(muted));
                });
            },
        );

        ui.add_space(pad_lg);
        ui.label(
            egui::RichText::new(page_label)
                .size(f_body)
                .strong()
                .color(primary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(pad_lg);
            if let Some(i) = seg_with_label(
                ui,
                &seg,
                f_micro,
                "Theme",
                &[
                    ("Mocha", theme_id == ThemeId::Mocha),
                    ("Latte", theme_id == ThemeId::Latte),
                ],
            ) {
                act_theme = Some(if i == 0 {
                    ThemeId::Mocha
                } else {
                    ThemeId::Latte
                });
            }
            ui.add_space(pad_lg);
            let stops = ui_scale_stops();
            let scale_items: Vec<(&str, bool)> = stops
                .iter()
                .map(|(l, s)| (l.as_str(), (ui_scale - s).abs() < 0.001))
                .collect();
            if let Some(i) = seg_with_label(ui, &seg, f_micro, "Scale", &scale_items) {
                act_scale = Some(stops[i].1);
            }
            ui.add_space(pad_lg);
            if let Some(i) = seg_with_label(
                ui,
                &seg,
                f_micro,
                "Specs",
                &[("Off", !specs_on), ("On", specs_on)],
            ) {
                act_specs = Some(i == 1);
            }
        });
    });

    let rect = ui.max_rect();
    let stroke = egui::Stroke::new(seg.border_w, separator);
    ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
    ui.painter()
        .vline(rect.left() + NAV_WIDTH.value(), rect.y_range(), stroke);

    if let Some(id) = act_theme {
        state.switch_theme(id);
    }
    if let Some(on) = act_specs {
        state.specs_on = on;
    }
    if let Some(s) = act_scale
        && (state.ui_scale - s).abs() >= 0.001
    {
        state.ui_scale = s;
        state.needs_reapply = true;
    }
}

/// 좌상단 brand 로고 텍스처를 1회 디코드/업로드 후 캐시에서 핸들 반환.
fn brand_logo_texture(
    ctx: &egui::Context,
    cache: &mut Option<egui::TextureHandle>,
) -> egui::TextureHandle {
    cache
        .get_or_insert_with(|| {
            ctx.load_texture(
                "brand_logo",
                decode_brand_logo(),
                egui::TextureOptions::LINEAR,
            )
        })
        .clone()
}

/// 임베드한 256×256 앱 아이콘 PNG 를 egui 이미지로 디코드 (cwd 무관).
fn decode_brand_logo() -> egui::ColorImage {
    static ICON_PNG: &[u8] = include_bytes!("../../../assets/icons/icon_256.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
    let mut reader = decoder.read_info().expect("builtin brand icon PNG decodes");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("brand icon buffer size")];
    let info = reader
        .next_frame(&mut buf)
        .expect("brand icon frame decodes");
    buf.truncate(info.buffer_size());
    egui::ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &buf)
}

/// 페이지 목록과 현재 페이지의 구역 앵커를 표시한다.
fn nav_ui(ui: &mut egui::Ui, state: &mut GalleryState) {
    let (on_active, muted, secondary, surf_active, separator) = {
        let t = &state.theme;
        (
            egui::Color32::from(t.text_on_active()),
            egui::Color32::from(t.text_muted()),
            egui::Color32::from(t.text_secondary()),
            egui::Color32::from(t.surface_active()),
            t.separator.to_egui_premultiplied(),
        )
    };
    let f_heading = state.theme.font_size_micro.value();
    let f_lbl = state.theme.font_size_body.value();
    let f_desc = state.theme.font_size_micro.value();
    let radius_sm = state.theme.corner_radius_sm.value();
    let pad_sm = state.theme.spacing_sm.value();
    let pad_lg = state.theme.spacing_lg.value();
    let row_h = state.theme.item_height_interactive.value();
    let border_w = state.theme.border_width.value();
    let margin_h = state.theme.spacing_sm.value() as i8;
    let margin_v = state.theme.spacing_xs.value() as i8;

    let active = state.active_page;
    let pages: Vec<(&'static str, &'static str)> = state
        .pages
        .iter()
        .map(|p| (p.category.label(), p.category.desc()))
        .collect();
    let sections: Vec<&'static str> = state
        .pages
        .get(active)
        .map(|p| p.sections.iter().map(|s| s.title).collect())
        .unwrap_or_default();

    let mut act_page: Option<usize> = None;

    egui::ScrollArea::vertical()
        .id_salt("g_nav_scroll")
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .show(ui, |ui| {
            nav_heading(ui, "Catalog", f_heading, muted);
            for (i, (lbl, desc)) in pages.iter().enumerate() {
                if nav_link(
                    ui,
                    NavLinkStyle {
                        on_active,
                        muted,
                        surf_active,
                        radius: radius_sm,
                        font_lbl: f_lbl,
                        font_desc: f_desc,
                        margin_h,
                        margin_v,
                    },
                    lbl,
                    desc,
                    i == active,
                ) {
                    act_page = Some(i);
                }
            }

            ui.add_space(pad_lg);
            nav_heading(ui, "On this page", f_heading, muted);
            for title in &sections {
                ui.horizontal(|ui| {
                    ui.add_space(pad_lg);
                    let (r, _) = ui.allocate_exact_size(
                        egui::vec2(border_w, row_h * 0.8),
                        egui::Sense::hover(),
                    );
                    ui.painter().vline(
                        r.center().x,
                        r.y_range(),
                        egui::Stroke::new(border_w, separator),
                    );
                    ui.add_space(pad_sm);
                    ui.label(egui::RichText::new(*title).size(f_lbl).color(secondary));
                });
            }
        });

    if let Some(i) = act_page
        && i != state.active_page
    {
        state.active_page = i;
    }
}

/// 우측 main — 활성 페이지 문서. pagehead + Section/Spec 트리.
fn main_ui(ui: &mut egui::Ui, state: &GalleryState) {
    let theme = &state.theme;
    let Some(page) = state.pages.get(state.active_page) else {
        ui.label("No page.");
        return;
    };

    // 넓은 예제가 ScrollArea의 가용 폭을 늘려도 본문이 밀리지 않도록 뷰포트 폭으로 중앙 정렬한다.
    let viewport_w = ui.available_width();
    let content_w = viewport_w.min(PAGE_MAX_WIDTH.value());
    let side = ((viewport_w - content_w) / 2.0).max(0.0);
    // 디자인의 좌우 여백 40px을 공용 토큰으로 계산한다.
    let pad_x = theme.spacing_xl.value() + theme.spacing_lg.value();

    let mut main_scroll = egui::ScrollArea::vertical()
        .id_salt("g_main_scroll")
        .auto_shrink([false, false])
        .drag_to_scroll(false);
    if let Some(y) = state.shot_scroll {
        main_scroll = main_scroll.vertical_scroll_offset(y);
    }
    main_scroll.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(side + pad_x);
            ui.vertical(|ui| {
                let col_w = (content_w - pad_x * 2.0).max(0.0);
                ui.set_max_width(col_w);
                // 넓은 예제에 영향을 받지 않도록 note의 줄바꿈 폭을 따로 전달한다.
                ui.data_mut(|d| d.insert_temp(crate::catalog::spec::body_column_width_id(), col_w));
                ui.add_space(theme.spacing_xl.value() + theme.spacing_md.value());
                page_head(ui, theme, page.category);
                for sec in &page.sections {
                    crate::catalog::spec::section(ui, theme, sec.title);
                    for sp in &sec.specs {
                        crate::catalog::spec::spec(ui, theme, sp.title, sp.when);
                        // 여러 예제의 내부 위젯 ID가 충돌하지 않도록 구분한다.
                        ui.push_id(sp.id, |ui| (sp.draw)(ui, theme));
                    }
                }
                ui.add_space(theme.spacing_xl.value() * 4.0);
            });
        });
    });
}

/// pagehead — h1 + intro + (Foundations 만) HowTo 3컬럼 배너.
fn page_head(ui: &mut egui::Ui, theme: &Theme, category: Category) {
    ui.label(
        egui::RichText::new(category.label())
            .size(theme.font_size_prose_h1.value())
            .strong()
            .color(egui::Color32::from(theme.text_primary())),
    );
    ui.add_space(theme.spacing_sm.value());
    ui.label(
        egui::RichText::new(category.intro())
            .size(theme.font_size_max.value())
            .color(egui::Color32::from(theme.text_secondary())),
    );
    if category.howto() {
        ui.add_space(theme.spacing_lg.value());
        let cells = [
            (
                "Usage",
                "Examples use shared widgets or reproduce layouts with Theme values.",
            ),
            (
                "Tokens used",
                "Shared values use Theme tokens; example-only dimensions are named locally.",
            ),
            (
                "Specs toggle",
                "The toggle is shown; the grid overlay is not implemented.",
            ),
        ];
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_panel()))
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                theme.separator.to_egui_premultiplied(),
            ))
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
            .show(ui, |ui| {
                ui.columns(3, |cols| {
                    for (i, (k, v)) in cells.iter().enumerate() {
                        cols[i].label(
                            egui::RichText::new(k.to_uppercase())
                                .size(theme.font_size_micro.value())
                                .color(egui::Color32::from(theme.accent_primary())),
                        );
                        cols[i].add_space(theme.spacing_xs.value());
                        cols[i].label(
                            egui::RichText::new(*v)
                                .size(theme.font_size_term_sm.value())
                                .color(egui::Color32::from(theme.text_secondary())),
                        );
                    }
                });
            });
    }
}

fn nav_heading(ui: &mut egui::Ui, text: &str, size: f32, color: egui::Color32) {
    ui.add_space(size);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(size)
            .color(color),
    );
    ui.add_space(size * 0.4);
}

#[derive(Clone, Copy)]
struct NavLinkStyle {
    on_active: egui::Color32,
    muted: egui::Color32,
    surf_active: egui::Color32,
    radius: f32,
    font_lbl: f32,
    font_desc: f32,
    margin_h: i8,
    margin_v: i8,
}

/// nav Catalog 링크 한 줄 — 좌 lbl + 우 desc, 활성 시 surface-active 배경.
fn nav_link(ui: &mut egui::Ui, s: NavLinkStyle, lbl: &str, desc: &str, active: bool) -> bool {
    let fg = if active { s.on_active } else { s.muted };
    let bg = if active {
        s.surf_active
    } else {
        egui::Color32::TRANSPARENT
    };
    let frame = egui::Frame::new()
        .fill(bg)
        .corner_radius(s.radius)
        .inner_margin(egui::Margin::symmetric(s.margin_h, s.margin_v));
    let resp = frame
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(lbl).size(s.font_lbl).color(fg));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(desc).size(s.font_desc).color(s.muted));
                });
            });
        })
        .response;
    let resp = resp.interact(egui::Sense::click());
    resp.clicked()
}

#[derive(Clone, Copy)]
struct SegStyle {
    border: egui::Color32,
    surf_raised: egui::Color32,
    surf_active: egui::Color32,
    on_active: egui::Color32,
    muted: egui::Color32,
    radius: f32,
    border_w: f32,
    height: f32,
    font: f32,
}

/// 라벨 붙은 세그 — mono micro 라벨 + 세그 컨트롤. 클릭된 옵션 index 반환.
fn seg_with_label(
    ui: &mut egui::Ui,
    s: &SegStyle,
    label_font: f32,
    label: &str,
    items: &[(&str, bool)],
) -> Option<usize> {
    // right_to_left 컨텍스트라 세그를 먼저, 라벨을 그 좌측에 둔다.
    let clicked = seg(ui, s, items);
    ui.label(
        egui::RichText::new(label.to_uppercase())
            .size(label_font)
            .color(s.muted),
    );
    clicked
}

/// 세그먼트 컨트롤 — border 1px + radius, 버튼들 사이 border-left.
fn seg(ui: &mut egui::Ui, s: &SegStyle, items: &[(&str, bool)]) -> Option<usize> {
    let mut clicked = None;
    egui::Frame::new()
        .fill(s.surf_raised)
        .stroke(egui::Stroke::new(s.border_w, s.border))
        .corner_radius(s.radius)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                for (i, (label, active)) in items.iter().enumerate() {
                    if i > 0 {
                        let (r, _) = ui.allocate_exact_size(
                            egui::vec2(s.border_w, s.height),
                            egui::Sense::hover(),
                        );
                        ui.painter().vline(
                            r.center().x,
                            r.y_range(),
                            egui::Stroke::new(s.border_w, s.border),
                        );
                    }
                    let fg = if *active { s.on_active } else { s.muted };
                    let bg = if *active {
                        s.surf_active
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let btn = egui::Button::new(egui::RichText::new(*label).size(s.font).color(fg))
                        .fill(bg)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(egui::CornerRadius::ZERO)
                        .min_size(egui::vec2(0.0, s.height));
                    if ui.add(btn).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_name_picks_the_builtin_theme_and_anything_else_none() {
        assert_eq!(ThemeId::from_name("latte"), Some(ThemeId::Latte));
        assert_eq!(ThemeId::from_name(" Latte "), Some(ThemeId::Latte));
        assert_eq!(ThemeId::from_name("MOCHA"), Some(ThemeId::Mocha));
        assert_eq!(ThemeId::from_name("frappe"), None);
        assert_eq!(ThemeId::from_name(""), None);
    }

    /// Mocha 에서 Latte 로 바꾸면 Latte 의 밝기(light)와 그 밝기의 accent 위 잉크를 쓴다.
    /// 되돌리면 다시 Mocha 의 dark 다.
    #[test]
    fn switching_theme_takes_the_new_themes_own_brightness() {
        let mut state = GalleryState::new();
        assert!(!state.theme.is_light);

        state.switch_theme(ThemeId::Latte);
        assert!(
            state.theme.is_light,
            "Latte kept the dark brightness of Mocha"
        );
        assert_eq!(
            state.theme.text_on_accent(),
            latte_theme().text_on_accent(),
            "accent ink after Mocha to Latte"
        );
        assert!(state.needs_reapply);

        state.switch_theme(ThemeId::Mocha);
        assert!(
            !state.theme.is_light,
            "Mocha kept the light brightness of Latte"
        );
        assert_eq!(
            state.theme.text_on_accent(),
            tasty_themes::mocha_fallback().text_on_accent()
        );
    }
}
