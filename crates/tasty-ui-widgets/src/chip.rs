//! 상호작용 없는 태그·배지·키캡. Theme의 컴포넌트 토큰으로 크기와 색을 정한다.
//! 글꼴 굵기는 별도 계열을 등록하지 않아 재현하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::control::Ground;

/// Tag variant (디자인 `core/Tag`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TagVariant {
    /// 외곽선 chip (기본) — tag-bg + tag-border + tag-fg.
    Default,
    Accent,
    Agent,
    /// 정보 태그. git-viewer와 원격 미러 워크스페이스에서 사용한다.
    Info,
    Success,
    Warning,
    Danger,
    /// 다른 클라이언트가 점유한 항목.
    Attached,
}

/// Badge variant (디자인 `core/Badge`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BadgeVariant {
    /// 채움 danger (기본 — unread count).
    Danger,
    /// Completion attention 개수(파랑).
    Primary,
    /// NeedsInput attention 개수(노랑).
    Warning,
    Agent,
    Success,
    Neutral,
}

fn mono(size: f32) -> egui::FontId {
    egui::FontId::monospace(size)
}

/// disabled 행 문맥을 기록하는 egui 임시 데이터 키. 값은 중첩 깊이와 행이 놓인 바탕이다.
fn disabled_chip_scope_id() -> egui::Id {
    egui::Id::new("tasty_ui_widgets::chip::disabled_chip_scope")
}

/// `content`를 disabled 문맥(시안 `.is-disabled` 행)에서 그린다.
/// 이 안의 [`tag`]·[`badge`]·[`badge_dot`]은 variant와 관계없이 disabled 변형으로 그린다.
/// disabled ListCtrl 행이 trailing 슬롯에 적용한다. `ground`는 행이 놓인 바탕이다.
pub fn disabled_chip_scope<R>(
    ui: &mut egui::Ui,
    ground: Ground,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let id = disabled_chip_scope_id();
    let prev = ui.ctx().data(|d| d.get_temp::<(u32, Ground)>(id));
    let depth = prev.map_or(0, |(depth, _)| depth);
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, (depth + 1, ground)));
    let out = content(ui);
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, prev.unwrap_or((0, Ground::Panel))));
    out
}

/// 지금 그리는 위치가 [`disabled_chip_scope`] 안인지. 직접 그리는 trailing renderer가
/// 자기 글자·글리프를 disabled ink로 바꿀 때 읽는다.
pub fn in_disabled_chip_scope(ui: &egui::Ui) -> bool {
    ui.ctx()
        .data(|d| d.get_temp::<(u32, Ground)>(disabled_chip_scope_id()))
        .is_some_and(|(depth, _)| depth > 0)
}

/// [`disabled_chip_scope`]가 받은 바탕. disabled Tag의 테두리가 바탕마다 다르다.
fn disabled_chip_ground(ui: &egui::Ui) -> Ground {
    ui.ctx()
        .data(|d| d.get_temp::<(u32, Ground)>(disabled_chip_scope_id()))
        .map_or(Ground::Panel, |(_, ground)| ground)
}

/// 상태 점 없는 태그의 폭을 계산한다. 그리기 전 가용 폭과 비교할 때 사용한다.
pub fn tag_width(ui: &egui::Ui, theme: &Theme, label: &str) -> f32 {
    tag_width_in(ui, theme, label, TagFace::label())
}

/// [`tag_mono`]의 폭을 계산한다.
pub fn tag_mono_width(ui: &egui::Ui, theme: &Theme, label: &str) -> f32 {
    tag_width_in(ui, theme, label, TagFace::mono())
}

fn tag_width_in(ui: &egui::Ui, theme: &Theme, label: &str, face: TagFace) -> f32 {
    let pad_x = theme.tag_padding_x().value();
    let galley = tag_galley(ui, theme, label, &face);
    galley.rect.width() + 2.0 * pad_x
}

/// Tag 라벨 글자의 형태.
struct TagFace {
    /// 낱말 Tag는 `font-label`, 값 Tag는 `font-mono`.
    family: egui::FontFamily,
    /// 대문자 Tag의 caps 자간. 나머지는 자간 없이 그린다.
    tracking: Option<LogicalPx>,
    /// 글자가 넘치면 말줄임하는 폭.
    max_text_w: Option<f32>,
}

impl TagFace {
    fn label() -> Self {
        Self {
            family: crate::label_font_family(),
            tracking: None,
            max_text_w: None,
        }
    }

    fn mono() -> Self {
        Self {
            family: egui::FontFamily::Monospace,
            ..Self::label()
        }
    }
}

/// Tag 라벨 galley.
fn tag_galley(
    ui: &egui::Ui,
    theme: &Theme,
    label: &str,
    face: &TagFace,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        label,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(theme.tag_font_size().value(), face.family.clone()),
            extra_letter_spacing: face.tracking.map_or(0.0, |t| t.value()),
            color: egui::Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    if let Some(w) = face.max_text_w {
        job.wrap = egui::text::TextWrapping::truncate_at_width(w.max(0.0));
    }
    ui.painter().layout_job(job)
}

/// Tag — 낱말을 `font-label`로 그리는 라벨 chip. `dot` 이 true 면 선행 상태 점(현재 fg 색).
/// [`disabled_chip_scope`] 안에서는 [`tag_disabled`]와 같게 그린다.
pub fn tag(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    variant: TagVariant,
    dot: bool,
) -> egui::Response {
    let colors = if in_disabled_chip_scope(ui) {
        tag_disabled_colors(theme, disabled_chip_ground(ui))
    } else {
        tag_colors(theme, variant)
    };
    paint_tag(
        ui,
        theme,
        label,
        TagLead::dot(dot),
        colors,
        TagFace::label(),
    )
}

/// 값 Tag(디자인 `Tag mono`) — ID·버전·크기·권한처럼 모든 언어에서 고정폭인 값을 담는다.
/// 색과 disabled 문맥 처리는 [`tag`]와 같다.
pub fn tag_mono(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    variant: TagVariant,
    dot: bool,
) -> egui::Response {
    let colors = if in_disabled_chip_scope(ui) {
        tag_disabled_colors(theme, disabled_chip_ground(ui))
    } else {
        tag_colors(theme, variant)
    };
    paint_tag(ui, theme, label, TagLead::dot(dot), colors, TagFace::mono())
}

/// [`tag`]와 같고 라벨 앞에 글자색 글리프를 둔다.
pub fn tag_with_icon(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    variant: TagVariant,
    icon: tasty_icons::Icon,
) -> egui::Response {
    let colors = if in_disabled_chip_scope(ui) {
        tag_disabled_colors(theme, disabled_chip_ground(ui))
    } else {
        tag_colors(theme, variant)
    };
    paint_tag(
        ui,
        theme,
        label,
        TagLead::Icon(icon),
        colors,
        TagFace::label(),
    )
}

/// 대문자 Tag(디자인 `Tag caps`) — 라벨을 대문자로 바꾸고 `letter-spacing-caps` 자간으로 그린다.
/// 대문자로 그리는 Tag는 모두 이 함수를 쓴다. 색과 disabled 문맥 처리는 [`tag`]와 같다.
pub fn tag_caps(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    variant: TagVariant,
    dot: bool,
) -> egui::Response {
    let colors = if in_disabled_chip_scope(ui) {
        tag_disabled_colors(theme, disabled_chip_ground(ui))
    } else {
        tag_colors(theme, variant)
    };
    let tracking = theme.letter_spacing_caps(theme.tag_font_size());
    paint_tag(
        ui,
        theme,
        &label.to_uppercase(),
        TagLead::dot(dot),
        colors,
        TagFace {
            tracking: Some(tracking),
            ..TagFace::label()
        },
    )
}

/// disabled Tag — 모든 variant가 중립 상자와 disabled ink를 쓴다. `ground`는 Tag가 놓인 바탕이며
/// 테두리가 그 바탕을 따른다. accent 채움과 tint 테두리는 빠지고 상태 점도 같은 ink다.
/// 조건을 알리는 Tag에는 쓰지 않는다. 그런 문구는 비활성 컨트롤이 아니라 기본 [`tag`]다.
pub fn tag_disabled(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    dot: bool,
    ground: Ground,
) -> egui::Response {
    paint_tag(
        ui,
        theme,
        label,
        TagLead::dot(dot),
        tag_disabled_colors(theme, ground),
        TagFace::label(),
    )
}

/// 기본 [`tag`]를 `max_w`(Tag 전체 폭) 안에 그린다. 라벨이 넘치면 말줄임한다(시안
/// `maxWidth: 100%` + ellipsis). 전체 라벨 툴팁은 호출자가 붙인다.
pub fn tag_truncated(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    max_w: LogicalPx,
) -> egui::Response {
    let text_w = max_w.value() - 2.0 * theme.tag_padding_x().value();
    paint_tag(
        ui,
        theme,
        label,
        TagLead::None,
        tag_colors(theme, TagVariant::Default),
        TagFace {
            max_text_w: Some(text_w),
            ..TagFace::label()
        },
    )
}

/// 라벨 앞에 오는 표지. 점과 글리프 모두 글자색으로 그린다.
enum TagLead {
    None,
    Dot,
    Icon(tasty_icons::Icon),
}

impl TagLead {
    fn dot(dot: bool) -> Self {
        if dot { Self::Dot } else { Self::None }
    }
}

/// Tag 채움·테두리·글자색.
type TagColors = (egui::Color32, Option<egui::Color32>, egui::Color32);

fn tag_disabled_colors(theme: &Theme, ground: Ground) -> TagColors {
    let edge = match ground {
        Ground::Panel => theme.tag_disabled_border(),
        Ground::Raised => theme.border_strong(),
        Ground::Active => theme.border_frame(),
    };
    (
        theme.tag_disabled_bg().to_egui(),
        Some(edge.to_egui()),
        theme.tag_disabled_fg().to_egui(),
    )
}

pub(crate) fn tag_colors(theme: &Theme, variant: TagVariant) -> TagColors {
    // 상태 variant 테두리의 color-mix 비율. 대응 토큰이 없다.
    const TAG_BORDER_OPACITY: f32 = 0.4;
    match variant {
        TagVariant::Default => (
            theme.tag_bg().to_egui(),
            Some(theme.tag_border().to_egui()),
            theme.tag_fg().to_egui(),
        ),
        TagVariant::Accent => (
            theme.accent_primary().to_egui(),
            None,
            theme.text_on_accent().to_egui(),
        ),
        TagVariant::Agent => (
            theme.accent_agent().to_egui(),
            None,
            theme.text_on_accent().to_egui(),
        ),
        TagVariant::Info => (
            theme.tag_bg().to_egui(),
            Some(
                theme
                    .accent_info()
                    .to_egui()
                    .gamma_multiply(TAG_BORDER_OPACITY),
            ),
            theme.tag_info_fg().to_egui(),
        ),
        TagVariant::Success => (
            theme.tag_bg().to_egui(),
            Some(
                theme
                    .accent_success()
                    .to_egui()
                    .gamma_multiply(TAG_BORDER_OPACITY),
            ),
            theme.tag_success_fg().to_egui(),
        ),
        TagVariant::Warning => (
            theme.tag_bg().to_egui(),
            Some(
                theme
                    .accent_warning()
                    .to_egui()
                    .gamma_multiply(TAG_BORDER_OPACITY),
            ),
            theme.tag_warning_fg().to_egui(),
        ),
        TagVariant::Danger => (
            theme.tag_bg().to_egui(),
            Some(
                theme
                    .accent_danger()
                    .to_egui()
                    .gamma_multiply(TAG_BORDER_OPACITY),
            ),
            theme.tag_danger_fg().to_egui(),
        ),
        TagVariant::Attached => (
            theme.tag_bg().to_egui(),
            Some(
                theme
                    .border_attached()
                    .to_egui()
                    .gamma_multiply(TAG_BORDER_OPACITY),
            ),
            theme.tag_attached_fg().to_egui(),
        ),
    }
}

fn paint_tag(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    lead: TagLead,
    (fill, border, fg): TagColors,
    face: TagFace,
) -> egui::Response {
    let radius = theme.tag_radius().value();
    let bw = theme.border_width.value();
    let pad_x = theme.tag_padding_x().value();
    let gap = theme.tag_gap().value();
    let dot_sz = theme.tag_dot_size().value();
    let tag_h = theme.tag_size().value();
    let galley = tag_galley(ui, theme, label, &face);
    let icon_sz = theme.icon_glyph_size_xs.value();
    let lead_w = match lead {
        TagLead::None => 0.0,
        TagLead::Dot => dot_sz + gap,
        TagLead::Icon(_) => icon_sz + gap,
    };
    let w = galley.rect.width() + lead_w + 2.0 * pad_x;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, tag_h), egui::Sense::hover());
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, radius, fill);
    }
    if let Some(bc) = border {
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(bw, bc),
            egui::StrokeKind::Inside,
        );
    }
    let mut x = rect.left() + pad_x;
    match lead {
        TagLead::None => {}
        TagLead::Dot => {
            let c = egui::pos2(x + dot_sz * 0.5, rect.center().y);
            ui.painter().circle_filled(c, dot_sz * 0.5, fg);
            x += dot_sz + gap;
        }
        TagLead::Icon(icon) => {
            let r = egui::Rect::from_min_size(
                egui::pos2(x, rect.center().y - icon_sz * 0.5),
                egui::vec2(icon_sz, icon_sz),
            );
            icon.image(icon_sz, fg).paint_at(ui, r);
            x += icon_sz + gap;
        }
    }
    let pos = egui::pos2(x, rect.center().y - galley.rect.height() * 0.5);
    ui.painter().galley(pos, galley, fg);
    resp
}

/// Badge — 채움 count/status pill (디자인 `core/Badge`).
/// [`disabled_chip_scope`] 안에서는 [`badge_disabled`]와 같게 그린다.
pub fn badge(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    variant: BadgeVariant,
) -> egui::Response {
    let colors = if in_disabled_chip_scope(ui) {
        badge_disabled_colors(theme)
    } else {
        badge_colors(theme, variant)
    };
    paint_badge(ui, theme, label, colors)
}

/// disabled Badge — 모든 variant가 중립 채움(badge-disabled-bg)과 disabled ink를 쓴다.
pub fn badge_disabled(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    paint_badge(ui, theme, label, badge_disabled_colors(theme))
}

fn badge_disabled_colors(theme: &Theme) -> (egui::Color32, egui::Color32) {
    (
        theme.badge_disabled_bg().to_egui(),
        theme.badge_disabled_fg().to_egui(),
    )
}

fn badge_colors(theme: &Theme, variant: BadgeVariant) -> (egui::Color32, egui::Color32) {
    match variant {
        BadgeVariant::Danger => (
            theme.badge_danger_bg().to_egui(),
            theme.badge_danger_fg().to_egui(),
        ),
        BadgeVariant::Primary => (
            theme.badge_primary_bg().to_egui(),
            theme.badge_primary_fg().to_egui(),
        ),
        BadgeVariant::Warning => (
            theme.badge_warning_bg().to_egui(),
            theme.badge_warning_fg().to_egui(),
        ),
        BadgeVariant::Agent => (
            theme.badge_agent_bg().to_egui(),
            theme.badge_agent_fg().to_egui(),
        ),
        BadgeVariant::Success => (
            theme.badge_success_bg().to_egui(),
            theme.badge_success_fg().to_egui(),
        ),
        BadgeVariant::Neutral => (
            theme.badge_neutral_bg().to_egui(),
            theme.badge_neutral_fg().to_egui(),
        ),
    }
}

fn paint_badge(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    (fill, fg): (egui::Color32, egui::Color32),
) -> egui::Response {
    let pad_x = theme.badge_padding_x().value();
    let badge_sz = theme.badge_size().value();
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        mono(theme.badge_font_size().value()),
        egui::Color32::PLACEHOLDER,
    );
    let w = (galley.rect.width() + 2.0 * pad_x).max(badge_sz);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, badge_sz), egui::Sense::hover());
    // radius-pill = 완전 둥금 (badge-radius 는 radius-sm 이나 구현은 pill idiom 유지).
    ui.painter().rect_filled(rect, badge_sz * 0.5, fill);
    let pos = rect.center() - galley.rect.size() * 0.5;
    ui.painter().galley(pos, galley, fg);
    resp
}

/// 상태 점의 영역을 할당한 뒤 공용 그리기 함수를 호출한다.
/// [`disabled_chip_scope`] 안에서는 badge-disabled-bg로 칠한다.
pub fn badge_dot(ui: &mut egui::Ui, theme: &Theme, variant: BadgeVariant) -> egui::Response {
    let dot_sz = theme.badge_dot_size().value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(dot_sz, dot_sz), egui::Sense::hover());
    if in_disabled_chip_scope(ui) {
        ui.painter().circle_filled(
            rect.center(),
            dot_sz * 0.5,
            theme.badge_disabled_bg().to_egui(),
        );
    } else {
        paint_badge_dot(ui.painter(), theme, rect.center(), variant);
    }
    resp
}

/// 이미 계산된 좌표에 상태 점을 그린다. 크기는 배율이 적용된 badge-dot-size를 사용한다.
pub fn paint_badge_dot(
    painter: &egui::Painter,
    theme: &Theme,
    center: egui::Pos2,
    variant: BadgeVariant,
) {
    let fill = match variant {
        BadgeVariant::Danger => theme.badge_danger_bg().to_egui(),
        BadgeVariant::Primary => theme.badge_primary_bg().to_egui(),
        BadgeVariant::Warning => theme.badge_warning_bg().to_egui(),
        BadgeVariant::Agent => theme.badge_agent_bg().to_egui(),
        BadgeVariant::Success => theme.badge_success_bg().to_egui(),
        BadgeVariant::Neutral => theme.surface_active().to_egui(),
    };
    painter.circle_filled(center, theme.badge_dot_size().value() * 0.5, fill);
}

/// 숫자 키캡의 영역을 할당한 뒤 공용 그리기 함수를 호출한다.
pub fn num_keycap(ui: &mut egui::Ui, theme: &Theme, digit: &str, active: bool) -> egui::Response {
    let side = theme.switch_overlay_size().value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    paint_num_keycap(ui.painter(), theme, rect.center(), digit, active, 1.0);
    resp
}

/// 이미 계산된 좌표에 숫자 키캡을 그린다.
/// alpha는 채움·테두리·글자에 함께 적용한다. 크기·색은 switch-overlay 토큰을,
/// 대응 값이 없는 반경·글꼴은 kbd 토큰을 사용한다.
pub fn paint_num_keycap(
    painter: &egui::Painter,
    theme: &Theme,
    center: egui::Pos2,
    digit: &str,
    active: bool,
    alpha: f32,
) {
    let (fill, border, fg) = if active {
        (
            theme.switch_overlay_active_bg().to_egui(),
            theme.switch_overlay_active_bg().to_egui(),
            theme.switch_overlay_active_fg().to_egui(),
        )
    } else {
        (
            theme.switch_overlay_bg().to_egui(),
            theme.switch_overlay_border().to_egui(),
            theme.switch_overlay_fg().to_egui(),
        )
    };
    let (fill, border, fg) = (
        fill.gamma_multiply(alpha),
        border.gamma_multiply(alpha),
        fg.gamma_multiply(alpha),
    );
    let side = theme.switch_overlay_size().value();
    let radius = theme.kbd_radius().value();
    let bw = theme.border_width.value();
    let bottom_border = theme.switch_overlay_shadow_depth().value();
    let rect = egui::Rect::from_center_size(center, egui::vec2(side, side));
    painter.rect_filled(rect, radius, fill);
    // 키캡 하단 보더 2px 강조 → 윗변 1px, 아랫변 2px 로 따로 그린다 (kbd 와 동일).
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, border),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            egui::pos2(rect.left() + radius, rect.bottom() - bw),
            egui::pos2(rect.right() - radius, rect.bottom() - bw),
        ],
        egui::Stroke::new(bottom_border, border),
    );
    let galley = painter.layout_no_wrap(
        digit.to_owned(),
        mono(theme.kbd_font_size().value()),
        egui::Color32::PLACEHOLDER,
    );
    let pos = rect.center() - galley.rect.size() * 0.5;
    painter.galley(pos, galley, fg);
}

/// Kbd — 키캡 시퀀스. `keys` 는 `"+"` 로 분할(예: `"Ctrl+K"`), 각 키를 키캡으로.
/// `+` 키 자체는 [`split_keys`] 규칙으로 키캡 하나가 된다(`"+"`, `"Ctrl++"`).
/// `ground`는 키캡이 놓인 바탕이다.
pub fn kbd(ui: &mut egui::Ui, theme: &Theme, keys: &str, ground: Ground) {
    let owned: Vec<KbdKey<'_>> = split_keys(keys).into_iter().map(KbdKey::Text).collect();
    kbd_parts(ui, theme, &owned, ground);
}

/// 키 조합 문자열을 키 이름으로 나눈다. 구분자와 같은 `+` 키는 분할 결과에 빈 조각 두 개로
/// 나타나므로(`"+"` → `["", ""]`, `"Ctrl++"` → `["Ctrl", "", ""]`) 연속한 빈 조각을 `+` 키 하나로 합친다.
pub(crate) fn split_keys(keys: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut segs = keys.split('+').peekable();
    while let Some(seg) = segs.next() {
        if seg.is_empty() && segs.peek() == Some(&"") {
            segs.next();
            out.push("+");
        } else {
            out.push(seg);
        }
    }
    out
}

/// 글자 폭에 패딩을 더하되 정사각 최소 크기를 유지한다.
fn cap_width(text_w: f32, pad_x: f32, kbd_h: f32) -> f32 {
    (text_w + 2.0 * pad_x).max(kbd_h)
}

/// 키캡과 사이의 + 라벨 폭을 그리는 순서로 반환한다. 항목 사이 간격은 제외한다.
fn kbd_item_widths(ctx: &egui::Context, theme: &Theme, keys: &[KbdKey<'_>]) -> Vec<f32> {
    let micro = theme.kbd_font_size().value();
    let pad_x = theme.kbd_padding_x().value();
    let kbd_h = theme.kbd_size().value();
    let measure = |text: &str| {
        ctx.fonts(|f| {
            f.layout_no_wrap(text.to_owned(), mono(micro), egui::Color32::PLACEHOLDER)
                .rect
                .width()
        })
    };
    let mut out = Vec::with_capacity(keys.len().saturating_mul(2).saturating_sub(1));
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            out.push(measure("+"));
        }
        out.push(match key {
            KbdKey::Text(text) => cap_width(measure(text), pad_x, kbd_h),
            KbdKey::Icon(_) => kbd_h,
        });
    }
    out
}

/// 키캡을 그릴 때와 같은 식으로 항목 폭과 간격을 합산한다.
/// 그리기 전에 상태바·팔레트의 남은 공간을 계산할 때 사용한다.
pub fn kbd_parts_width(ctx: &egui::Context, theme: &Theme, keys: &[KbdKey<'_>]) -> LogicalPx {
    let items = kbd_item_widths(ctx, theme, keys);
    let gaps = items.len().saturating_sub(1) as f32;
    LogicalPx(items.iter().sum::<f32>() + theme.kbd_gap().value() * gaps)
}

/// [`kbd`] 가 차지할 폭 — [`split_keys`] 로 나눈 뒤 [`kbd_parts_width`] 에 넘긴다.
pub fn kbd_width(ctx: &egui::Context, theme: &Theme, keys: &str) -> LogicalPx {
    let parts: Vec<KbdKey<'_>> = split_keys(keys).into_iter().map(KbdKey::Text).collect();
    kbd_parts_width(ctx, theme, &parts)
}

/// 키캡의 텍스트 또는 아이콘. 폰트에 없는 보조 키 문자는 SVG 아이콘으로 전달할 수 있다.
pub enum KbdKey<'a> {
    Text(&'a str),
    Icon(tasty_icons::Icon),
}

/// 키캡 사이 `+`의 색. 키캡 상자는 자기 채움을 가지므로 바탕을 따르는 것은 `+`뿐이다.
/// 공용 표의 선택 행은 셀 내용이 행 상태를 모르므로 표 문맥을 함께 읽는다.
fn kbd_plus_ink(ui: &egui::Ui, theme: &Theme, ground: Ground) -> egui::Color32 {
    if ground == Ground::Active || crate::in_selected_table_row(ui) {
        theme.kbd_plus_fg_active().to_egui()
    } else {
        theme.kbd_plus_fg().to_egui()
    }
}

/// 텍스트·아이콘 키캡을 함께 그린다. 두 종류는 같은 패딩·반경·테두리를 사용한다.
pub fn kbd_parts(ui: &mut egui::Ui, theme: &Theme, keys: &[KbdKey<'_>], ground: Ground) {
    let radius = theme.kbd_radius().value();
    let bw = theme.border_width.value();
    let border = theme.kbd_border().to_egui();
    let fill = theme.kbd_bg().to_egui();
    let fg = theme.kbd_fg().to_egui();
    let plus = kbd_plus_ink(ui, theme, ground);
    let micro = theme.kbd_font_size().value();
    let icon_glyph = theme.icon_glyph_size_sm.value();
    let gap = theme.kbd_gap().value();
    let pad_x = theme.kbd_padding_x().value();
    let kbd_h = theme.kbd_size().value();
    let bottom_border = theme.kbd_shadow_depth().value();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                ui.label(egui::RichText::new("+").size(micro).color(plus).monospace());
            }
            match key {
                KbdKey::Text(text) => {
                    let galley = ui.painter().layout_no_wrap(
                        (*text).to_owned(),
                        mono(micro),
                        egui::Color32::PLACEHOLDER,
                    );
                    let w = cap_width(galley.rect.width(), pad_x, kbd_h);
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(w, kbd_h), egui::Sense::hover());
                    draw_keycap_box(ui.painter(), rect, radius, bw, fill, border, bottom_border);
                    let pos = rect.center() - galley.rect.size() * 0.5;
                    ui.painter().galley(pos, galley, fg);
                }
                KbdKey::Icon(icon) => {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(kbd_h, kbd_h), egui::Sense::hover());
                    draw_keycap_box(ui.painter(), rect, radius, bw, fill, border, bottom_border);
                    let irect = egui::Rect::from_center_size(
                        rect.center(),
                        egui::vec2(icon_glyph, icon_glyph),
                    );
                    icon.image(icon_glyph, fg).paint_at(ui, irect);
                }
            }
        }
    });
}

/// 공용 폭 계산으로 키캡을 좌표에 직접 그린다.
/// right_x에서 왼쪽으로 배치하고 center_y에 세로 중심을 맞춘 뒤 사용한 폭을 반환한다.
pub fn kbd_parts_at(
    ui: &egui::Ui,
    theme: &Theme,
    keys: &[KbdKey<'_>],
    right_x: f32,
    center_y: f32,
    ground: Ground,
) -> LogicalPx {
    let plus = kbd_plus_ink(ui, theme, ground);
    kbd_parts_at_ink(ui, theme, keys, right_x, center_y, None, plus)
}

/// `kbd_parts_at`과 같지만 `key_ink`가 있으면 키 글자·글리프를 그 색으로 그리고 `+`는 `plus`로 그린다.
/// disabled 메뉴 항목은 둘 다 disabled ink로 바꾼다. 키보드 활성 행은 행 채움 위에 놓이는 `+`만 바꾼다.
pub(crate) fn kbd_parts_at_ink(
    ui: &egui::Ui,
    theme: &Theme,
    keys: &[KbdKey<'_>],
    right_x: f32,
    center_y: f32,
    key_ink: Option<egui::Color32>,
    plus: egui::Color32,
) -> LogicalPx {
    let total = kbd_parts_width(ui.ctx(), theme, keys);
    if keys.is_empty() {
        return total;
    }
    let radius = theme.kbd_radius().value();
    let bw = theme.border_width.value();
    let border = theme.kbd_border().to_egui();
    let fill = theme.kbd_bg().to_egui();
    let fg = key_ink.unwrap_or_else(|| theme.kbd_fg().to_egui());
    let micro = theme.kbd_font_size().value();
    let icon_glyph = theme.icon_glyph_size_sm.value();
    let gap = theme.kbd_gap().value();
    let pad_x = theme.kbd_padding_x().value();
    let kbd_h = theme.kbd_size().value();
    let bottom_border = theme.kbd_shadow_depth().value();
    let top = center_y - kbd_h * 0.5;
    let mut x = right_x - total.value();
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            let g = ui
                .painter()
                .layout_no_wrap("+".to_owned(), mono(micro), plus);
            let w = g.rect.width();
            let pos = egui::pos2(x, center_y - g.rect.height() * 0.5);
            ui.painter().galley(pos, g, plus);
            x += w + gap;
        }
        match key {
            KbdKey::Text(text) => {
                let g = ui.painter().layout_no_wrap(
                    (*text).to_owned(),
                    mono(micro),
                    egui::Color32::PLACEHOLDER,
                );
                let w = cap_width(g.rect.width(), pad_x, kbd_h);
                let rect = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(w, kbd_h));
                draw_keycap_box(ui.painter(), rect, radius, bw, fill, border, bottom_border);
                let pos = rect.center() - g.rect.size() * 0.5;
                ui.painter().galley(pos, g, fg);
                x += w + gap;
            }
            KbdKey::Icon(icon) => {
                let rect = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(kbd_h, kbd_h));
                draw_keycap_box(ui.painter(), rect, radius, bw, fill, border, bottom_border);
                let irect =
                    egui::Rect::from_center_size(rect.center(), egui::vec2(icon_glyph, icon_glyph));
                icon.image(icon_glyph, fg).paint_at(ui, irect);
                x += kbd_h + gap;
            }
        }
    }
    total
}

/// `kbd_parts_at`과 같은 키캡을 `Ui` 없이 `painter`에 그린다. 텍스트 키만 받는다.
/// 상자·글자·`+` 색에 `alpha`를 곱해 페이드하는 카드(토스트 hint) 안에서 쓴다.
/// 오른쪽 끝을 `right_x`, 세로 중심을 `center_y`에 맞추고 차지한 폭을 돌려준다.
pub fn kbd_text_parts_painted(
    painter: &egui::Painter,
    theme: &Theme,
    keys: &[&str],
    right_x: f32,
    center_y: f32,
    alpha: f32,
) -> LogicalPx {
    let parts: Vec<KbdKey<'_>> = keys.iter().map(|k| KbdKey::Text(k)).collect();
    let total = kbd_parts_width(painter.ctx(), theme, &parts);
    if keys.is_empty() {
        return total;
    }
    let radius = theme.kbd_radius().value();
    let bw = theme.border_width.value();
    let border: egui::Color32 = theme.kbd_border().gamma_multiply(alpha).into();
    let fill: egui::Color32 = theme.kbd_bg().gamma_multiply(alpha).into();
    let fg: egui::Color32 = theme.kbd_fg().gamma_multiply(alpha).into();
    let plus: egui::Color32 = theme.kbd_plus_fg().gamma_multiply(alpha).into();
    let micro = theme.kbd_font_size().value();
    let gap = theme.kbd_gap().value();
    let pad_x = theme.kbd_padding_x().value();
    let kbd_h = theme.kbd_size().value();
    let bottom_border = theme.kbd_shadow_depth().value();
    let top = center_y - kbd_h * 0.5;
    let mut x = right_x - total.value();
    for (i, text) in keys.iter().enumerate() {
        if i > 0 {
            let g = painter.layout_no_wrap("+".to_owned(), mono(micro), plus);
            let w = g.rect.width();
            let pos = egui::pos2(x, center_y - g.rect.height() * 0.5);
            painter.galley(pos, g, plus);
            x += w + gap;
        }
        let g = painter.layout_no_wrap((*text).to_owned(), mono(micro), egui::Color32::PLACEHOLDER);
        let w = cap_width(g.rect.width(), pad_x, kbd_h);
        let rect = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(w, kbd_h));
        draw_keycap_box(painter, rect, radius, bw, fill, border, bottom_border);
        let pos = rect.center() - g.rect.size() * 0.5;
        painter.galley(pos, g, fg);
        x += w + gap;
    }
    total
}

/// 텍스트·아이콘 키캡이 공유하는 배경과 아래쪽 강조 테두리.
#[allow(clippy::too_many_arguments)]
fn draw_keycap_box(
    painter: &egui::Painter,
    rect: egui::Rect,
    radius: f32,
    bw: f32,
    fill: egui::Color32,
    border: egui::Color32,
    bottom_border: f32,
) {
    painter.rect_filled(rect, radius, fill);
    // 키캡 하단 보더 2px 강조 → 윗변은 1px, 아랫변은 2px 로 따로 그린다.
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, border),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            egui::pos2(rect.left() + radius, rect.bottom() - bw),
            egui::pos2(rect.right() - radius, rect.bottom() - bw),
        ],
        egui::Stroke::new(bottom_border, border),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_plus_key_is_one_keycap() {
        assert_eq!(super::split_keys("+"), vec!["+"]);
        assert_eq!(super::split_keys("Ctrl++"), vec!["Ctrl", "+"]);
        assert_eq!(
            super::split_keys("Ctrl+Shift+K"),
            vec!["Ctrl", "Shift", "K"]
        );
        assert_eq!(super::split_keys("="), vec!["="]);
    }

    use super::*;

    /// 디자인 Tag는 variant와 관계없이 tag-bg 위에 그리고 글자만 variant별로 다르다.
    /// 틴트 채움을 쓰면 대비 측정값이 맞지 않는다.
    #[test]
    fn status_tags_paint_tag_bg_with_their_own_ink() {
        let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).unwrap();
        let (partial, is_light) = file.to_partial();
        let mut colors = tasty_themes::mocha_fallback_colors();
        colors.apply_partial(&partial);
        let latte = Theme::with_colors(colors, is_light.unwrap_or(true));
        for theme in [tasty_themes::mocha_fallback(), latte] {
            let cases = [
                (TagVariant::Info, theme.tag_info_fg()),
                (TagVariant::Success, theme.tag_success_fg()),
                (TagVariant::Warning, theme.tag_warning_fg()),
                (TagVariant::Danger, theme.tag_danger_fg()),
            ];
            for (variant, ink) in cases {
                let (fill, border, fg) = tag_colors(&theme, variant);
                assert_eq!(fill, theme.tag_bg().to_egui());
                assert_eq!(fg, ink.to_egui());
                assert!(border.is_some(), "status tag keeps its accent edge");
            }
        }
    }

    /// 그려진 Tag 라벨과 그 자간을 모은다.
    fn tag_texts(draw: impl Fn(&mut egui::Ui)) -> Vec<(String, f32)> {
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| draw(ui));
        });
        output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => Some((
                    t.galley.text().to_string(),
                    t.galley.job.sections[0].format.extra_letter_spacing,
                )),
                _ => None,
            })
            .collect()
    }

    /// 상태 variant는 어느 행에 놓여도 같은 Tag 채움 위에 자기 글자색을 쓴다.
    #[test]
    fn the_attached_tag_uses_the_tag_recipe() {
        let theme = tasty_themes::mocha_fallback();
        let (fill, border, fg) = tag_colors(&theme, TagVariant::Attached);
        assert_eq!(fill, theme.tag_bg().to_egui());
        assert!(border.is_some());
        assert_eq!(fg, theme.tag_attached_fg().to_egui());
    }

    /// 대문자 Tag만 라벨을 대문자로 바꾸고 caps 자간을 그린다. 일반 Tag는 받은 그대로다.
    #[test]
    fn caps_tag_uppercases_and_spaces_the_label() {
        let theme = tasty_themes::mocha_fallback();
        let caps = theme.letter_spacing_caps(theme.tag_font_size()).value();
        assert!(caps > 0.0);
        let texts = tag_texts(|ui| {
            tag_caps(ui, &theme, "remote", TagVariant::Info, false);
            tag(ui, &theme, "plain", TagVariant::Default, false);
        });
        assert_eq!(
            texts,
            vec![("REMOTE".to_string(), caps), ("plain".to_string(), 0.0)]
        );
    }

    #[test]
    fn disabled_chip_scope_marks_only_its_own_scope_and_nests() {
        let ctx = egui::Context::default();
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!in_disabled_chip_scope(ui));
                disabled_chip_scope(ui, Ground::Panel, |ui| {
                    assert!(in_disabled_chip_scope(ui));
                    disabled_chip_scope(ui, Ground::Panel, |ui| {
                        assert!(in_disabled_chip_scope(ui))
                    });
                    assert!(in_disabled_chip_scope(ui));
                });
                assert!(!in_disabled_chip_scope(ui));
            });
        });
    }

    /// disabled 변형은 variant와 관계없이 중립 상자와 disabled ink 한 벌이다. accent 채움이 남지 않는다.
    #[test]
    fn disabled_variants_drop_accent_fill_and_tint_edge() {
        let theme = tasty_themes::mocha_fallback();
        let (fill, border, fg) = tag_disabled_colors(&theme, Ground::Panel);
        assert_eq!(
            tag_disabled_colors(&theme, Ground::Raised).1,
            Some(theme.border_strong().to_egui())
        );
        assert_eq!(
            tag_disabled_colors(&theme, Ground::Active).1,
            Some(theme.border_frame().to_egui())
        );
        assert_eq!(fill, theme.state_disabled_fill().to_egui());
        assert_eq!(border, Some(theme.state_disabled_border().to_egui()));
        assert_eq!(fg, theme.state_disabled_fg().to_egui());
        for variant in [TagVariant::Accent, TagVariant::Success, TagVariant::Agent] {
            assert_ne!(tag_colors(&theme, variant).2, fg);
        }
        let (bfill, bfg) = badge_disabled_colors(&theme);
        assert_eq!(bfill, theme.state_disabled_fill().to_egui());
        assert_eq!(bfg, theme.state_disabled_fg().to_egui());
    }

    /// 문맥 안의 `tag`·`badge`는 호출부를 바꾸지 않아도 disabled 변형으로 그린다.
    #[test]
    fn tag_and_badge_inside_the_scope_paint_the_disabled_ink() {
        let theme = tasty_themes::mocha_fallback();
        let ink = theme.state_disabled_fg().to_egui();
        let accent = theme.accent_primary().to_egui();
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                disabled_chip_scope(ui, Ground::Panel, |ui| {
                    tag(ui, &theme, "edited", TagVariant::Accent, false);
                    badge(ui, &theme, "3", BadgeVariant::Primary);
                });
            });
        });
        let mut text_colors = Vec::new();
        let mut fills = Vec::new();
        for clipped in &output.shapes {
            match &clipped.shape {
                egui::Shape::Text(t) => text_colors.push(t.fallback_color),
                egui::Shape::Rect(r) => fills.push(r.fill),
                _ => {}
            }
        }
        assert_eq!(text_colors, vec![ink, ink]);
        assert!(!fills.contains(&accent), "accent fill left in {fills:?}");
    }
}
