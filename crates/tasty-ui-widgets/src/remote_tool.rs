//! 본체와 갤러리가 공유하는 원격 연결 화면의 그리기 함수.
//! 폼·필터·팝업 상태, 파일 읽기, 번역, 팝업 배치는 호출자가 맡는다.

use std::collections::HashSet;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::ControlSize;
use crate::tokens::{STRUCT_GAP_1, STRUCT_GAP_2};
use crate::vspace;
use crate::{TagVariant, tag};

/// 프로토콜 드롭다운 프레임 안쪽의 최소 폭. `remote-filter-dropdown-width` 는 테두리를 포함한
/// 폭(border-box)이므로 호출자가 그리는 프레임의 좌우 합(`frame_x`, 테두리 + 안쪽 여백)을 뺀다.
pub fn filter_dropdown_content_width(th: &Theme, frame_x: LogicalPx) -> LogicalPx {
    LogicalPx((th.remote_filter_dropdown_width().value() - frame_x.value()).max(0.0))
}
/// 프로토콜 목록이 길어질 때의 스크롤 상한. 드롭다운이 팝업 밖으로 자라지 않게 한다.
pub const FILTER_DROPDOWN_MAX_HEIGHT: LogicalPx = LogicalPx(168.0);

// 복사할 수 있지만 변경 내용은 저장하지 않는 텍스트다. 매 프레임 원본으로 버퍼를 만든다.

/// [`selectable_text`] 의 줄바꿈 모드.
#[derive(Clone, Copy)]
pub enum TextWrap {
    /// 줄바꿈 없음, 콘텐츠 폭만큼만 차지(`egui::Label` 기본값과 동형).
    None,
    /// 가용 폭에서 여러 줄로 줄바꿈(`Label::wrap()` 과 동형).
    Wrap,
    /// 지정 폭에서 한 줄로 줄인다. 폭은 egui LayoutJob에 직접 전달한다.
    Truncate(f32),
}

/// 선택·복사 가능한 텍스트를 그린다. 편집 결과는 보존하지 않는다.
pub fn selectable_text(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
    italic: bool,
    wrap: TextWrap,
) -> egui::Response {
    selectable_text_tracked(ui, text, color, size, monospace, italic, wrap, 0.0)
}

/// 선택 가능한 텍스트에 논리 픽셀 단위 자간을 추가한다.
#[allow(clippy::too_many_arguments)] // reason: 한 줄 텍스트의 표현 축이 그만큼이다
pub fn selectable_text_tracked(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
    italic: bool,
    wrap: TextWrap,
    tracking: f32,
) -> egui::Response {
    let color = color.into();
    let font_id = if monospace {
        egui::FontId::monospace(size)
    } else {
        egui::FontId::proportional(size)
    };
    let mut buffer = text.to_string();
    let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color,
                italics: italic,
                extra_letter_spacing: tracking,
                ..Default::default()
            },
        );
        match wrap {
            TextWrap::None => job.wrap.max_width = f32::INFINITY,
            TextWrap::Wrap => {
                job.wrap.max_width = wrap_width;
                job.break_on_newline = true;
            }
            TextWrap::Truncate(w) => {
                job.wrap.max_width = w;
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
            }
        }
        ui.fonts(|f| f.layout_job(job))
    };
    // Wrap만 가용 폭을 채운다. 나머지는 내용 폭을 사용해 짧은 텍스트가 정렬 공간을 독점하지 않게 한다.
    let desired_width = if matches!(wrap, TextWrap::Wrap) {
        f32::INFINITY
    } else {
        0.0
    };
    ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .frame(false)
            .desired_rows(1)
            .desired_width(desired_width)
            .layouter(&mut layouter),
    )
}

/// [`selectable_text`] 축약형 — 줄바꿈 없음·italic 아님(가장 흔한 경우).
pub fn selectable_label(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
) -> egui::Response {
    selectable_text(ui, text, color, size, monospace, false, TextWrap::None)
}

/// [`selectable_label`] 에 자간을 더한 것 — 대문자 섹션 헤딩용.
pub fn selectable_label_tracked(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
    tracking: f32,
) -> egui::Response {
    selectable_text_tracked(
        ui,
        text,
        color,
        size,
        monospace,
        false,
        TextWrap::None,
        tracking,
    )
}

// ── 구역 구분선 · 버튼 ───────────────────────────────────────────────────

/// 패널 배경과 구분되도록 surface-raised를 사용하는 보조 버튼.
pub fn secondary_button(ui: &mut egui::Ui, th: &Theme, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .color(th.text_primary())
                .size(th.font_size_body.value()),
        )
        .fill(th.surface_raised())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_strong(),
        )),
    )
}

/// Primary 버튼 — accent 채움 + on-accent 텍스트. 디자인 `Button variant="primary"`.
pub fn primary_button(ui: &mut egui::Ui, th: &Theme, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .color(th.text_on_accent())
                .size(th.font_size_body.value()),
        )
        .fill(th.accent_primary()),
    )
}

/// Ghost 버튼 — 투명 배경 + secondary 텍스트(hover 시 overlay). 디자인
/// `Button variant="ghost"`.
pub fn ghost_button(ui: &mut egui::Ui, th: &Theme, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .color(th.text_secondary())
                .size(th.font_size_body.value()),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE),
    )
}

/// 경고 배지 — 시안 `WarnBadge`. 경고 글리프(icon-glyph-xs)와 mono micro 글자를 담은 pill이며
/// 높이·좌우 여백·간격·모서리는 Tag 토큰, 채움은 accent-warning × `tint-fill-alpha`,
/// 테두리는 Tag 상태 테두리와 같은 40%다. `text`·`tooltip` 은 호출자가 번역해 넘긴다.
pub fn warn_badge(ui: &mut egui::Ui, th: &Theme, text: &str, tooltip: &str) -> egui::Response {
    let warn = th.accent_warning().to_egui();
    let galley = warn_badge_galley(ui, th, text);
    let (pad_x, glyph, gap) = warn_badge_metrics(th);
    let w = pad_x * 2.0 + glyph + gap + galley.rect.width();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(w, th.tag_size().value()), egui::Sense::hover());
    let radius = th.tag_radius().value();
    ui.painter()
        .rect_filled(rect, radius, warn.gamma_multiply(th.tint_fill_alpha()));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            th.border_width.value(),
            warn.gamma_multiply(WARN_BADGE_BORDER_OPACITY),
        ),
        egui::StrokeKind::Inside,
    );
    let glyph_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad_x, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    let ink = th.tag_warning_fg().to_egui();
    tasty_icons::ALERT_TRIANGLE
        .image(glyph, ink)
        .paint_at(ui, glyph_rect);
    let pos = egui::pos2(
        glyph_rect.right() + gap,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, ink);
    resp.on_hover_text(tooltip)
}

/// 경고 배지 테두리 계수. 시안 `color-mix(accent-warning 40%)` 이며 Tag 상태 테두리와 같다.
const WARN_BADGE_BORDER_OPACITY: f32 = 0.4;

/// (좌우 여백, 글리프 크기, 글리프와 글자 사이 간격).
fn warn_badge_metrics(th: &Theme) -> (f32, f32, f32) {
    (
        th.tag_padding_x().value(),
        th.icon_glyph_size_xs.value(),
        th.tag_gap().value(),
    )
}

fn warn_badge_galley(ui: &egui::Ui, th: &Theme, text: &str) -> std::sync::Arc<egui::Galley> {
    ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::monospace(th.tag_font_size().value()),
        egui::Color32::PLACEHOLDER,
    )
}

/// [`warn_badge`] 의 폭. 앞 칸의 말줄임 폭을 정할 때 뺀다.
pub fn warn_badge_width(ui: &egui::Ui, th: &Theme, text: &str) -> f32 {
    let (pad_x, glyph, gap) = warn_badge_metrics(th);
    pad_x * 2.0 + glyph + gap + warn_badge_galley(ui, th, text).rect.width()
}

// ── 목록 행 셸 ───────────────────────────────────────────────────────────

/// 행 오른쪽 동작 묶음의 폭. Sm 아이콘 버튼 `count` 개를 `size-1` 간격으로 둔다.
pub fn remote_row_actions_width(th: &Theme, count: usize) -> f32 {
    let n = count as f32;
    crate::ControlSize::Sm.height(th) * n + STRUCT_GAP_1.value() * (n - 1.0).max(0.0)
}

/// 원격 도구 세 탭(프로필·Attach·Passkeys)이 함께 쓰는 목록 행 셸. 시안 `ProfileRow`·
/// `AttachRow`·`PasskeyRow` 의 공통 틀이다. 위아래 `space-md`·좌우 `space-xs` 안쪽 여백,
/// 텍스트 열과 동작 묶음 사이 `space-sm`, 텍스트 줄 사이 `size-2`, 아래 1px `separator`.
/// 동작 묶음의 폭을 먼저 빼므로 긴 텍스트가 버튼을 밀어내지 않는다. `body` 는 텍스트 열의
/// 폭을 받아 줄을 그리고, `actions` 는 오른쪽부터(삭제가 맨 오른쪽) 버튼을 그린다.
pub fn remote_list_row(
    ui: &mut egui::Ui,
    th: &Theme,
    action_count: usize,
    body: impl FnOnce(&mut egui::Ui, f32),
    actions: impl FnOnce(&mut egui::Ui),
) {
    let pad_x = th.spacing_xs.value();
    let gap = th.spacing_sm.value();
    vspace(ui, th.spacing_md);
    let actions_w = remote_row_actions_width(th, action_count);
    let button_h = ControlSize::Sm.height(th);
    let full = ui.available_rect_before_wrap();
    let text_w = (full.width() - 2.0 * pad_x - gap - actions_w).max(0.0);
    // 동작 묶음은 행 오른쪽 끝의 고정 사각형에 둔다. 텍스트 칸이 넓어져도 버튼 위치는 그대로다.
    let actions_rect = egui::Rect::from_min_size(
        egui::pos2(full.right() - pad_x - actions_w, full.top()),
        egui::vec2(actions_w, button_h),
    );
    ui.horizontal_top(|ui| {
        // add_space 는 항목 간격을 더하지 않으므로 왼쪽 여백이 그대로 pad_x 다.
        ui.add_space(pad_x);
        ui.allocate_ui_with_layout(
            egui::vec2(text_w, button_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(text_w);
                ui.set_min_height(button_h);
                ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
                body(ui, text_w);
            },
        );
    });
    let mut actions_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(actions_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Min)),
    );
    actions_ui.spacing_mut().item_spacing.x = STRUCT_GAP_1.value();
    actions(&mut actions_ui);
    vspace(ui, th.spacing_md);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

/// 행 첫 줄 끝에 붙는 칩.
pub enum RemoteRowChip<'a> {
    /// 낱말 Tag(Attach 방식).
    Tag(&'a str),
    /// 값 Tag(프로필 type, passkey kind).
    TagMono(&'a str),
    /// [`warn_badge`] — 모르는 type·kind, 비활성, 참조 없음.
    Warn { text: &'a str, tooltip: &'a str },
}

/// 행 첫 줄. 이름(body · text-primary, 흐린 행은 text-disabled)과 선택 라벨
/// (`  (label)`, text-muted)을 한 칸에 두고 칩 폭을 뺀 나머지에서 끝을 말줄임한다.
/// 시안의 이름 weight 600은 egui UI에 굵은 글꼴이 없어 색으로만 근사한다
/// ([디자인 정합 지침]의 폰트 weight 항목).
pub fn remote_row_title(
    ui: &mut egui::Ui,
    th: &Theme,
    name: &str,
    label: Option<&str>,
    dim: bool,
    chips: &[RemoteRowChip<'_>],
    width: f32,
) {
    ui.horizontal(|ui| {
        let gap = th.spacing_sm.value();
        ui.spacing_mut().item_spacing.x = gap;
        let chips_w: f32 = chips
            .iter()
            .map(|c| match c {
                RemoteRowChip::Tag(t) => crate::tag_width(ui, th, t) + gap,
                RemoteRowChip::TagMono(t) => crate::tag_mono_width(ui, th, t) + gap,
                RemoteRowChip::Warn { text, .. } => warn_badge_width(ui, th, text) + gap,
            })
            .sum();
        let name_w = (width - chips_w).max(0.0);
        let name_color = if dim {
            th.text_muted()
        } else {
            th.text_primary()
        };
        let label = label.filter(|l| !l.is_empty());
        let text = match label {
            Some(l) => format!("{name}  ({l})"),
            None => name.to_string(),
        };
        title_text(
            ui,
            th,
            &text,
            name.len(),
            name_color.to_egui(),
            th.text_muted().to_egui(),
            name_w,
        );
        for c in chips {
            match c {
                RemoteRowChip::Tag(t) => {
                    tag(ui, th, t, TagVariant::Default, false);
                }
                RemoteRowChip::TagMono(t) => {
                    crate::tag_mono(ui, th, t, TagVariant::Default, false);
                }
                RemoteRowChip::Warn { text, tooltip } => {
                    warn_badge(ui, th, text, tooltip);
                }
            }
        }
    });
}

/// 앞 `split` 바이트는 `head`, 나머지는 `tail` 색으로 그리는 선택 가능한 한 줄. 폭을 넘으면
/// 끝을 말줄임한다.
fn title_text(
    ui: &mut egui::Ui,
    th: &Theme,
    text: &str,
    split: usize,
    head: egui::Color32,
    tail: egui::Color32,
    max_w: f32,
) -> egui::Response {
    let font_id = egui::FontId::proportional(th.font_size_body.value());
    let job_for = move |text: &str| {
        let mut job = egui::text::LayoutJob::default();
        let cut = split.min(text.len());
        let cut = (0..=cut)
            .rev()
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0);
        for (part, color) in [(&text[..cut], head), (&text[cut..], tail)] {
            job.append(
                part,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color,
                    ..Default::default()
                },
            );
        }
        job.wrap.max_width = max_w;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job
    };
    // 칸 폭은 말줄임한 글자 폭이다. 짧은 이름이 뒤 칩을 오른쪽 끝으로 밀지 않게 한다.
    let content_w = ui.fonts(|f| f.layout_job(job_for(text))).size().x;
    let mut buffer = text.to_string();
    let mut layouter = move |ui: &egui::Ui, text: &str, _wrap_width: f32| {
        ui.fonts(|f| f.layout_job(job_for(text)))
    };
    ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .frame(false)
            .desired_rows(1)
            .desired_width(content_w)
            .layouter(&mut layouter),
    )
}

// ── 3탭 언더라인 탭 스트립 ───────────────────────────────────────────────

/// 화면을 전환하는 밑줄 탭의 입력. 값을 고르는 segmented와 구분한다.
pub struct TabStripData<'a> {
    /// 탭 라벨(이미 번역된 것). 개수가 곧 탭 수다.
    pub labels: &'a [&'a str],
    /// 지금 켜진 탭의 인덱스.
    pub active: usize,
    /// 바 배경과 하단 separator 가 채울 가로 범위 — popup 전체폭이다. 탭 자체는
    /// 이 범위의 왼쪽에서 시작한다.
    pub x_range: egui::Rangef,
}

/// 클릭한 다른 탭의 인덱스를 반환한다. 현재 탭을 다시 누르면 None이다.
pub fn draw_tab_strip(ui: &mut egui::Ui, th: &Theme, data: &TabStripData<'_>) -> Option<usize> {
    // 디자인 remote_tool.jsx TabBtn: 전체폭 bg-sidebar(mantle), height 35,
    // padding L8 / TabBtn padding 0 13 / gap 2.
    let tab_h = 36.0; // 디자인 TabBtn 35 + borderBottom 1 = 탭바 컨테이너 36
    let pad_l = 8.0; // 디자인 탭바 padding-left
    let pad_x = 13.0; // 디자인 TabBtn padding 0 13
    let gap = 2.0; // 디자인 탭바 gap
    let font = egui::FontId::proportional(th.font_size_body.value());

    let top = ui.cursor().top();
    let bar = egui::Rect::from_min_size(
        egui::pos2(data.x_range.min, top),
        egui::vec2(data.x_range.span(), tab_h),
    );
    // bg-sidebar 전체폭 + 하단 borderBottom separator (mantle 위 → surface1 근사).
    ui.painter().rect_filled(bar, 0.0, th.bg_sidebar());
    ui.painter().hline(
        data.x_range,
        bar.max.y,
        egui::Stroke::new(th.border_width.value(), th.border_strong()),
    );

    let mut clicked = None;
    let mut x = data.x_range.min + pad_l;
    for (i, label) in data.labels.iter().enumerate() {
        let on = i == data.active;
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap((*label).to_string(), font.clone(), th.text_primary().into())
                .size()
                .x
        });
        let w = text_w + pad_x * 2.0;
        let rect = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(w, tab_h));
        let resp = ui.interact(rect, ui.id().with((i, "rt_tab")), egui::Sense::click());
        if resp.hovered() && !on {
            ui.painter()
                .rect_filled(rect, 0.0, th.hover_overlay.to_egui_premultiplied());
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font.clone(),
            if on {
                th.text_primary().into()
            } else {
                th.text_muted().into()
            },
        );
        // 활성 밑줄은 구분선 위에 그린다. 두 선의 굵기는 UI 배율과 무관하다.
        if on {
            ui.painter().hline(
                rect.x_range(),
                bar.max.y - th.border_width.value(),
                egui::Stroke::new(th.tab_indicator_width().value(), th.accent_primary()),
            );
        }
        if resp.clicked() && !on {
            clicked = Some(i);
        }
        x += w + gap;
    }
    ui.allocate_rect(bar, egui::Sense::hover());
    clicked
}

// SSH 설정 항목은 프로필 목록 아래 별도 구역으로 표시한다.
// 이미 가져온 호스트는 버튼 대신 상태 태그를 표시하며, 설정 없음은 오류색으로 표시하지 않는다.

/// 로컬 `~/.ssh/config` 행 하나 — tasty 레코드가 아니라 사용자 파일의 항목이다.
pub struct LocalSshHost<'a> {
    /// `Host` 별칭.
    pub alias: &'a str,
    /// 표시용 요약(`user@host:port`). 표시 전용 — 가져오기에는 alias 만 쓴다.
    pub target: &'a str,
    /// 이미 프로필로 가져왔는가. 그러면 액션 대신 Tag 가 붙는다.
    pub in_profiles: bool,
}

/// 섹션 한 프레임 분의 입력. 문자열은 전부 호출자가 번역해 넘긴다.
pub struct LocalSshSectionData<'a> {
    /// 섹션 제목. 그리기 직전에 대문자로 바꾼다(canonical `textTransform: uppercase`).
    pub heading: &'a str,
    /// 표시용 config 경로(`~/.ssh/config`).
    pub path: &'a str,
    /// 이미 가져온 호스트에 붙는 Tag 라벨.
    pub in_profiles_tag: &'a str,
    /// 미가져온 호스트의 ghost 액션 라벨.
    pub add_label: &'a str,
    /// 호스트가 0 건일 때의 한 줄 — 원인(없음/못 읽음/정말 0 건)은 호출자가 가른다.
    pub empty_message: &'a str,
    pub hosts: &'a [LocalSshHost<'a>],
}

/// 본문만 안쪽으로 들여쓴다. 구분선에는 이 여백을 적용하지 않는다.
fn ssh_inset<R>(ui: &mut egui::Ui, th: &Theme, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(th.spacing_xs.value() as i8, 0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// 프로필 아래에 SSH 설정 항목을 표시하고 가져오기를 누른 인덱스를 반환한다.
pub fn draw_local_ssh_section(
    ui: &mut egui::Ui,
    th: &Theme,
    data: &LocalSshSectionData<'_>,
) -> Option<usize> {
    let mut clicked = None;
    // 프로필과 별도 구역임을 구분선으로 표시한다.
    vspace(ui, th.spacing_md);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        egui::Stroke::new(th.border_width.value(), th.border_frame()),
    );
    vspace(ui, th.spacing_sm);

    // 헤더와 빈 상태 줄은 위 size-2 · 아래 space-xs 여백을 둔다.
    vspace(ui, STRUCT_GAP_2);
    ssh_inset(ui, th, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            selectable_label_tracked(
                ui,
                &data.heading.to_uppercase(),
                th.text_secondary(),
                th.font_size_caption.value(),
                false,
                th.letter_spacing_caps(th.font_size_caption).value(),
            );
            selectable_label(
                ui,
                data.path,
                th.text_muted(),
                th.font_size_caption.value(),
                true,
            );
            if !data.hosts.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    selectable_label(
                        ui,
                        &data.hosts.len().to_string(),
                        th.text_muted(),
                        th.font_size_caption.value(),
                        true,
                    );
                });
            }
        });
    });
    vspace(ui, th.spacing_xs);

    if data.hosts.is_empty() {
        vspace(ui, STRUCT_GAP_2);
        // 비어 있는 이유를 표시하되 설정 부재를 경고색으로 표현하지 않는다.
        ssh_inset(ui, th, |ui| {
            selectable_text(
                ui,
                data.empty_message,
                th.text_muted(),
                th.font_size_caption.value(),
                false,
                false,
                TextWrap::Wrap,
            );
        });
        vspace(ui, th.spacing_xs);
        return None;
    }
    for (i, h) in data.hosts.iter().enumerate() {
        if draw_local_ssh_row(ui, th, h, data) {
            clicked = Some(i);
        }
    }
    clicked
}

/// alias 행 한 줄 — 2 줄 본문 + 우측 Tag 또는 ghost 액션. 액션을 눌렀으면 `true`.
fn draw_local_ssh_row(
    ui: &mut egui::Ui,
    th: &Theme,
    h: &LocalSshHost<'_>,
    data: &LocalSshSectionData<'_>,
) -> bool {
    let mut clicked = false;
    vspace(ui, th.spacing_xs);
    ssh_inset(ui, th, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            // 긴 별칭이 오른쪽 버튼을 밀지 않도록 버튼 영역을 먼저 확보한다.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if h.in_profiles {
                    // 이미 가져온 항목은 재등록 버튼 대신 상태를 표시한다.
                    tag(ui, th, data.in_profiles_tag, TagVariant::Default, false);
                } else if crate::Button::new(data.add_label)
                    .variant(crate::ButtonVariant::Ghost)
                    .size(crate::ControlSize::Sm)
                    .leading_icon(&|ui, rect, c| {
                        tasty_icons::PLUS.image(rect.height(), c).paint_at(ui, rect)
                    })
                    .show(ui, th)
                    .clicked()
                {
                    clicked = true;
                }
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = th.label_detail_gap.value();
                    let w = ui.available_width();
                    selectable_text(
                        ui,
                        h.alias,
                        th.text_secondary(),
                        th.font_size_body.value(),
                        false,
                        false,
                        TextWrap::Truncate(w),
                    );
                    selectable_text(
                        ui,
                        h.target,
                        th.text_muted(),
                        th.font_size_caption.value(),
                        true,
                        false,
                        TextWrap::Truncate(w),
                    );
                });
            });
        });
    });
    vspace(ui, th.spacing_xs);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    clicked
}

// ── 프로토콜 필터 ────────────────────────────────────────────────────────

/// 필터 목록의 한 프로토콜.
pub struct ProtocolFilterItem<'a> {
    pub name: &'a str,
    /// tasty 가 전용 폼을 가진 kind 가 아님 — 경고 배지가 붙는다.
    pub unknown: bool,
}

/// 드롭다운 본문의 문자열(전부 번역된 것).
pub struct ProtocolFilterLabels<'a> {
    pub title: &'a str,
    pub select_all: &'a str,
    pub deselect_all: &'a str,
    pub reset: &'a str,
    pub apply: &'a str,
    pub unknown: &'a str,
    pub unknown_hint: &'a str,
}

/// 프로토콜 필터 버튼(funnel + 라벨). `filtered` 면 primary(accent), 아니면 secondary.
pub fn draw_protocol_filter_button(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    filtered: bool,
) -> egui::Response {
    let text_col: egui::Color32 = if filtered {
        th.text_on_accent().into()
    } else {
        th.text_primary().into()
    };
    let fill: egui::Color32 = if filtered {
        th.accent_primary_raised().into()
    } else {
        th.surface_raised().into()
    };
    let stroke = if filtered {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(th.border_width.value(), th.border_strong())
    };
    ui.add(
        egui::Button::image_and_text(
            tasty_icons::FUNNEL.image(th.icon_glyph_size_sm.value(), text_col),
            egui::RichText::new(label)
                .color(text_col)
                .size(th.font_size_body.value()),
        )
        .fill(fill)
        .stroke(stroke),
    )
}

/// 프로토콜 제외 집합 draft를 편집한다. 체크된 항목은 제외되지 않은 항목이다.
/// 전체 선택·해제·초기화는 draft를 즉시 바꾸며 적용 버튼만 true를 반환한다.
/// 팝업 배치·폭([`filter_dropdown_content_width`])과 실제 필터 적용은 호출자가 처리한다.
///
/// 시안 `ProtocolFilter` 처럼 제목 · 목록 · 일괄 선택 · Reset/Apply 네 구획을 위아래
/// space-sm, 좌우 space-md 여백으로 그리고 구획 사이에 separator 선을 둔다. 구획이 프레임
/// 끝까지 닿으므로 호출자는 안쪽 여백이 없는 프레임에 담는다.
pub fn draw_protocol_filter_body(
    ui: &mut egui::Ui,
    th: &Theme,
    items: &[ProtocolFilterItem<'_>],
    labels: &ProtocolFilterLabels<'_>,
    draft: &mut HashSet<String>,
) -> bool {
    let mut applied = false;
    ui.spacing_mut().item_spacing.y = 0.0;
    filter_section(ui, th, |ui| {
        // 목록·링크와 같은 왼쪽 선에서 시작하도록 TextEdit 여백이 없는 일반 라벨로 그린다.
        // 자간은 시안의 `letter-spacing-caps`(0.04em)다.
        let size = th.font_size_micro;
        let mut job = egui::text::LayoutJob::default();
        job.append(
            &labels.title.to_uppercase(),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::monospace(size.value()),
                color: th.text_muted().into(),
                extra_letter_spacing: th.letter_spacing_caps(size).value(),
                ..Default::default()
            },
        );
        ui.add(egui::Label::new(job).selectable(false));
    });
    filter_rule(ui, th);
    filter_section(ui, th, |ui| {
        // 바깥 높이가 좁은 자리(가로로 감싸는 행 등)에서도 상한까지 자라고, 내용이 짧으면 줄어든다.
        egui::ScrollArea::vertical()
            .max_height(FILTER_DROPDOWN_MAX_HEIGHT.value())
            .min_scrolled_height(FILTER_DROPDOWN_MAX_HEIGHT.value())
            .drag_to_scroll(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                for item in items {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                        let mut checked = !draft.contains(item.name);
                        if crate::checkbox(
                            ui,
                            th,
                            &mut checked,
                            item.name,
                            true,
                            crate::Ground::Raised,
                        )
                        .changed()
                        {
                            if checked {
                                draft.remove(item.name);
                            } else {
                                draft.insert(item.name.to_string());
                            }
                        }
                        if item.unknown {
                            warn_badge(ui, th, labels.unknown, labels.unknown_hint);
                        }
                    });
                }
            });
    });
    filter_rule(ui, th);
    filter_section(ui, th, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            if filter_link(ui, th, labels.select_all).clicked() {
                draft.clear();
            }
            ui.label(
                egui::RichText::new("·")
                    .color(th.separator.to_egui_premultiplied())
                    .size(th.font_size_caption.value()),
            );
            if filter_link(ui, th, labels.deselect_all).clicked() {
                *draft = items.iter().map(|i| i.name.to_string()).collect();
            }
        });
    });
    filter_rule(ui, th);
    filter_section(ui, th, |ui| {
        ui.horizontal(|ui| {
            if crate::Button::new(labels.reset)
                .variant(crate::ButtonVariant::Ghost)
                .size(crate::ControlSize::Sm)
                .show(ui, th)
                .clicked()
            {
                draft.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::Button::new(labels.apply)
                    .variant(crate::ButtonVariant::Primary)
                    .size(crate::ControlSize::Sm)
                    .show(ui, th)
                    .clicked()
                {
                    applied = true;
                }
            });
        });
    });
    applied
}

/// 필터 드롭다운의 한 구획 — 위아래 space-sm, 좌우 space-md.
fn filter_section(ui: &mut egui::Ui, th: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .inner_margin(crate::margin_sym(th.spacing_md, th.spacing_sm))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// 구획 사이의 separator 선. 드롭다운 폭 전체에 걸친다.
fn filter_rule(ui: &mut egui::Ui, th: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), th.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 0.0, th.separator.to_egui_premultiplied());
}

/// 시안 `rtLinkBtn` — 테두리·배경·여백 없는 accent 캡션 글자 버튼.
fn filter_link(ui: &mut egui::Ui, th: &Theme, label: &str) -> egui::Response {
    ui.add(
        egui::Label::new(
            egui::RichText::new(label)
                .color(th.accent_primary_raised())
                .size(th.font_size_caption.value()),
        )
        .selectable(false)
        .sense(egui::Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}
