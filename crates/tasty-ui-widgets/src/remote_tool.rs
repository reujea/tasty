//! Remote connections popup 의 **순수 시각** — 디자인
//! `ui_kits/terminal/overlays/remote_tool.jsx` 와 그 갤러리 미러
//! `gallery/overlays-shared.jsx` `RemoteFrame` / `LocalSshSection` 대응.
//!
//! ## 왜 이 crate 인가
//!
//! 갤러리(`tasty-gallery`)는 `tasty` bin 크레이트를 의존할 수 없다. 그래서 본체에만
//! 있는 그리기는 갤러리에서 **부를 수가 없고**, 전사 사본이 되면 두 자리가 말없이
//! 갈린다. 공용 crate 로 내리면 둘이 같은 함수를 부른다 —
//! [`crate::status_bar`] 의 `draw_status_bar_view` 가 같은 이유로 여기 있다.
//!
//! ## 이 crate 가 소유하지 않는 것 (=본체 wrapper 잔류)
//!
//! - **egui ctx memory** — 탭/폼 상태(`UiState`), 적용된 프로토콜 필터, 드롭다운
//!   열림 상태. 이 view 는 전부 인자로 받고 클릭만 돌려준다.
//! - **파일 IO** — `RemoteProfiles::load()` / `Passkeys::load()` / `~/.ssh/config`.
//! - **i18n** — 이 crate 는 `tasty-i18n` 을 의존하지 않는다(`status_bar`·`multi_select`
//!   와 동일 정책). 라벨·tooltip 문자열은 전부 props 로 주입받는다.
//! - **popup 배치·outside-click rect 보고** — 본체 popup 매니저 정책이다.

use std::collections::HashSet;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::tokens::{STRUCT_GAP_1, STRUCT_GAP_2};
use crate::vspace;
use crate::{TagVariant, tag};

/// 프로토콜 필터 드롭다운의 **내용** 최소 폭. 이 값을 정한 것은 이 공용 view 가 아니다 —
/// 팝업 파일에 있던 216 을 그대로 옮겼고, 216 은 확정 시안(`RemoteFrame` 의 filter 블록
/// `width: 236`)보다 앞서 정해졌다.
///
/// 좌변이 **내용** 폭이라 시안의 236(테두리 박스)과 직접 견줄 수 없다. 실측으로 견준다
/// — scale 1.0 · 창 1280×720 에서 드롭다운을 열고 테두리 색 `(204, 208, 218)` 이 나타나는
/// 열을 읽으면 테두리 박스는 x 874..1103 = **230 px** 이다. 내용이 216 이므로 프레임이
/// 먹는 폭은 좌우 합 **14 px** 이고, 시안과의 차는 20 이 아니라 **6 px** 이다. 종전의
/// "236 에서 좌우 패딩을 뺀 값" 이라는 설명은 그 14 를 20 으로 잡은 거짓이었다.
///
/// 그 6 px 을 어느 쪽으로 맞출지는 디자인이 정한다. 여기서 222 로 올리면 이 팝업의 픽셀이
/// 움직이는데, 픽셀 불변이 이 view 를 공용 크레이트로 옮긴 근거 자체다 — 그래서 값은 그대로
/// 두고 관측만 남긴다.
pub const FILTER_DROPDOWN_MIN_WIDTH: LogicalPx = LogicalPx(216.0);
/// 프로토콜 목록이 길어질 때의 스크롤 상한. 드롭다운이 팝업 밖으로 자라지 않게 한다.
pub const FILTER_DROPDOWN_MAX_HEIGHT: LogicalPx = LogicalPx(168.0);

// ── read-only selectable 텍스트 ──────────────────────────────────────────
//
// egui 의 `Label` 은 선택 복사가 안 되고 `TextEdit` 은 편집이 된다. 이 팝업의 값
// (호스트·포트·경로)은 **복사해서 쓰는 것**이 용건이라 선택은 필요하고 편집은 곤란하다.
// 그래서 매 프레임 지역 버퍼를 clone 해 넘긴다 — 타이핑해도 다음 프레임에 원래
// 텍스트로 돌아간다(편집 결과를 버린다).

/// [`selectable_text`] 의 줄바꿈 모드.
#[derive(Clone, Copy)]
pub enum TextWrap {
    /// 줄바꿈 없음, 콘텐츠 폭만큼만 차지(`egui::Label` 기본값과 동형).
    None,
    /// 가용 폭에서 여러 줄로 줄바꿈(`Label::wrap()` 과 동형).
    Wrap,
    /// 지정 폭에서 한 줄로 말줄임(`Label::truncate()` 과 동형) — `…` 로 elide.
    ///
    /// 폭이 `LogicalPx` 가 아닌 이유: 이 값은 곧장 egui `LayoutJob::wrap.max_width` 로
    /// 들어간다. 타입을 붙이면 만드는 자리 하나에서 벗기던 것을 쓰는 자리 하나에서
    /// 벗기게 될 뿐이라 총수가 그대로다.
    Truncate(f32),
}

/// selectable 텍스트 렌더 — 위 절 주석 참고.
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

/// [`selectable_text`] 에 **자간**(글자 사이 추가 간격, px)을 더한 것. 레이아웃 경로는
/// 하나뿐이고 `tracking = 0.0` 이 위의 기본이다 — 사본을 만들지 않으려고 이 쪽에 몰았다.
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
    // Wrap 은 가용 폭까지 확장해야 실제로 줄바꿈된다. None/Truncate 는 콘텐츠(또는
    // truncate 결과) 폭만큼만 차지해야 `Label` 과 동일하게 부모 레이아웃(가로 나열,
    // 우측 정렬용 RTL 트릭 등)에 자연스럽게 맞물린다 — desired_width 를 고정폭으로
    // 주면 짧은 텍스트도 그 폭을 다 차지해 정렬이 깨진다.
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

/// 구역 구분선 — egui `Separator` 는 이 팝업의 패널 배경 위에서 사실상 비가시라
/// `border_strong` 색 hline 으로 직접 그린다.
pub fn hsep(ui: &mut egui::Ui, th: &Theme) {
    vspace(ui, STRUCT_GAP_2);
    let r = ui.max_rect();
    ui.painter().hline(
        r.x_range(),
        ui.cursor().top(),
        egui::Stroke::new(th.border_width.value(), th.border_strong()),
    );
    vspace(ui, STRUCT_GAP_2);
}

/// 디자인 secondary 버튼 (`Button.jsx --secondary`): surface-raised(surface0) 채움.
/// base(bg-panel) 패널 배경 위에서 한 단계 밝게 떠 보인다. (egui inactive 기본 버튼은
/// fill=base 라 base 패널 위에서 묻히므로 fill 을 명시한다.)
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

/// 경고 배지 — 아이콘 없는 pill. `text`·`tooltip` 은 호출자가 번역해 넘긴다.
pub fn warn_badge(ui: &mut egui::Ui, th: &Theme, text: &str, tooltip: &str) {
    selectable_label(
        ui,
        &format!("⚠ {text}"),
        th.accent_warning(),
        th.font_size_caption.value(),
        false,
    )
    .on_hover_text(tooltip);
}

// ── 3탭 언더라인 탭 스트립 ───────────────────────────────────────────────

/// 언더라인 탭 스트립 한 프레임 분의 입력.
///
/// **밑줄이지 채움이 아니다** — 2026-09-17 디자인 결정(R1)이 둘을 갈랐다: 밑줄은
/// **view 를 바꾸고**(탭 스트립), 채움은 **값을 바꾼다**([`crate::segmented`]).
/// `surface-active` 는 행 선택 채움이라 어느 쪽도 아니다.
pub struct TabStripData<'a> {
    /// 탭 라벨(이미 번역된 것). 개수가 곧 탭 수다.
    pub labels: &'a [&'a str],
    /// 지금 켜진 탭의 인덱스.
    pub active: usize,
    /// 바 배경과 하단 separator 가 채울 가로 범위 — popup 전체폭이다. 탭 자체는
    /// 이 범위의 왼쪽에서 시작한다.
    pub x_range: egui::Rangef,
}

/// 언더라인 탭 스트립. 클릭된 탭 인덱스를 돌려준다(현재 탭 재클릭은 `None`).
///
/// 좌표를 직접 계산해 그린다(egui 자동 배치 우회) — 디자인 TabBtn 이 높이·패딩·gap 을
/// 고정값으로 정하고 있어 자동 배치로는 그 형상이 안 나온다.
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
        // 활성 탭 하단 accent bar — separator 위에 그려 덮는다. 굵기는 다른 탭
        // 지표(explorer/preset)와 같은 `tab_indicator_width`, 덮을 separator 두께는
        // `border_width`. 둘 다 얇은 구조선이라 zoom 을 타지 않는다(값 불변).
        if on {
            ui.painter().hline(
                rect.x_range(),
                bar.max.y - th.border_width.value(),
                egui::Stroke::new(th.tab_indicator_width.value(), th.accent_primary()),
            );
        }
        if resp.clicked() && !on {
            clicked = Some(i);
        }
        x += w + gap;
    }
    // 탭바 영역만큼 커서 전진 → 다음 구역(콘텐츠)이 그 아래로.
    ui.allocate_rect(bar, egui::Sense::hover());
    clicked
}

// ── 로컬 ssh config 섹션 ─────────────────────────────────────────────────
//
// 2026-09-17 디자인 결정(R2)이 이 섹션의 형상을 확정했다 — 원본은
// `gallery/overlays-shared.jsx` 의 `LocalSshSection`.
//
// **프로필 목록과 같은 스크롤 안에 두되, 한 tier 아래로 내린다.** 같은 물음("어느
// 기계냐")이라 탭을 나누면 숨고, 그렇다고 같은 층에 두면 저장된 프로필로 오인된다.
// 그 tier 차이를 네 가지가 만든다:
//   · 카드가 아니라 **섹션 헤더**(11px 대문자 라벨 + mono 경로 + 개수)
//   · 프로필 행의 3 줄이 아니라 **2 줄**(alias, 그리고 mono `user@host:port`)
//   · **행마다 있던 아이콘 버튼이 없다** — 우측 정렬 ghost "Add profile" 하나뿐이고
//     그것은 이미 있던 가져오기 동작이다(새 동작이 아니다)
//   · 이미 가져온 호스트는 muted Tag 만 달고 액션이 없다
// 비어 있음/못 읽음은 muted 한 줄씩이다 — **설정이 없는 것은 오류가 아니라서**
// warning 톤을 쓰지 않는다. tasty 는 이 파일을 읽기만 하고 쓰지 않는다.

/// 섹션 상단 여백 — canonical `marginTop: 10`. 그 값의 semantic 이 없다(spacing
/// 스텝은 4·8·12·16·24). 겨루는 component 토큰이 없어 이 출처가 곧 근거다 —
/// [`crate::status_bar`] 의 `BAR_PAD_X` 와 같은 사정이다.
const SSH_SECTION_MARGIN_TOP: LogicalPx = LogicalPx(10.0);
/// 헤더 라벨·경로·개수 사이 gap, 그리고 헤더/행의 세로 여백 — canonical `gap: 6` ·
/// `padding: "2px 4px 6px"` · `padding: "6px 4px"` 의 6. `size-6` 에 값은 있지만
/// 그것을 쓰는 component 토큰은 점의 지름(`status-dot-size-compact`) 하나뿐이라
/// 부르면 없는 관계가 생긴다.
const SSH_GAP: LogicalPx = LogicalPx(6.0);

/// 대문자 섹션 헤딩의 자간 — canonical `letterSpacing: "0.06em"`. em 이라 폰트 크기에
/// 비례하므로 px 상수가 아니라 비율로 들고, 그릴 때 폰트 크기를 곱한다(caption 11px 에서
/// 0.66px). 사이드바 섹션 헤딩이 같은 부류에 같은 축을 쓴다(`0.07em` = 10px 에서 0.7px).
const SECTION_HEADING_TRACKING_EM: f32 = 0.06;

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

/// canonical 의 `padding: … 4px` — 이 섹션의 본문만 프로필 행보다 한 칸 안쪽으로
/// 들여쓴다. "프로필 목록 아래 한 tier" 라는 관계를 들여쓰기로 말하는 자리이고,
/// 상단 rule 과 행 사이 separator 는 CSS border 라 padding 밖이므로 들여쓰지 않는다.
fn ssh_inset<R>(ui: &mut egui::Ui, th: &Theme, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(th.spacing_xs.value() as i8, 0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// 로컬 ssh config 섹션 — 프로필 목록 **아래**에 한 tier 내려 붙는다.
/// 가져오기를 누른 호스트의 인덱스를 돌려준다.
pub fn draw_local_ssh_section(
    ui: &mut egui::Ui,
    th: &Theme,
    data: &LocalSshSectionData<'_>,
) -> Option<usize> {
    let mut clicked = None;
    // 상단 rule — 프로필 목록과 가르는 선. 행 사이 `separator` 보다 한 단계 뚜렷한
    // `border-frame` 이라 "같은 목록의 다음 행" 이 아니라 "다른 구역" 으로 읽힌다.
    vspace(ui, SSH_SECTION_MARGIN_TOP);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        egui::Stroke::new(th.border_width.value(), th.border_frame()),
    );
    vspace(ui, th.spacing_sm);

    // 헤더 — 라벨 · 경로 · (호스트가 있으면) 개수.
    ssh_inset(ui, th, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SSH_GAP.value();
            selectable_label_tracked(
                ui,
                &data.heading.to_uppercase(),
                th.text_secondary(),
                th.font_size_caption.value(),
                false,
                th.font_size_caption.value() * SECTION_HEADING_TRACKING_EM,
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
    vspace(ui, SSH_GAP);

    if data.hosts.is_empty() {
        // 섹션을 통째로 숨기지 않는다 — 문구가 있어야 "가져올 게 없다" 와 "그런 기능이
        // 없다" 가 구분된다. 없는 설정은 오류가 아니므로 warning 톤이 아니다.
        ssh_inset(ui, th, |ui| {
            selectable_text(
                ui,
                data.empty_message,
                th.text_muted(),
                th.font_size_term_sm.value(),
                false,
                false,
                TextWrap::Wrap,
            );
        });
        vspace(ui, SSH_GAP);
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
    vspace(ui, SSH_GAP);
    ssh_inset(ui, th, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            // 우측 슬롯을 먼저 잡아 남는 폭을 본문이 쓴다 — 긴 alias 가 슬롯을 밀어내지
            // 않게 한다(canonical 의 `flex: 1; min-width: 0`).
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if h.in_profiles {
                    // 이미 가져온 호스트는 액션이 없다 — 같은 호스트를 두 번 등록하는
                    // 사고를 비활성 버튼이 아니라 **상태 표시**로 막는다.
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
                    ui.spacing_mut().item_spacing.y = STRUCT_GAP_1.value();
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
    vspace(ui, SSH_GAP);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        egui::Stroke::new(th.border_width.value(), th.separator),
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
        th.accent_primary().into()
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

/// 필터 드롭다운 **본문** — 체크박스 목록 + 모두선택/모두해제/초기화/적용.
///
/// 띄우는 것(popup 위젯·앵커·outside-click rect 보고)은 호출자 몫이다. 이 함수는
/// 넘겨받은 `ui` 안을 채우기만 한다 — 그래서 갤러리가 "열린 상태" 를 popup 없이
/// 카드 안에 그대로 그릴 수 있다.
///
/// `draft` 는 **제외 집합**이다(체크 = 미제외). 모두선택/모두해제/초기화는 여기서
/// 곧바로 `draft` 를 고치고, 적용만 `true` 로 보고한다 — Apply-on-confirm 이라
/// 적용 시점의 반영은 호출자가 한다.
pub fn draw_protocol_filter_body(
    ui: &mut egui::Ui,
    th: &Theme,
    items: &[ProtocolFilterItem<'_>],
    labels: &ProtocolFilterLabels<'_>,
    draft: &mut HashSet<String>,
) -> bool {
    let mut applied = false;
    ui.set_min_width(FILTER_DROPDOWN_MIN_WIDTH.value());
    selectable_label(
        ui,
        labels.title,
        th.text_muted(),
        th.font_size_caption.value(),
        true,
    );
    ui.add_space(th.spacing_xs.value());
    egui::ScrollArea::vertical()
        .max_height(FILTER_DROPDOWN_MAX_HEIGHT.value())
        .drag_to_scroll(false)
        .show(ui, |ui| {
            for item in items {
                ui.horizontal(|ui| {
                    // draft 는 제외 집합 → checked = 미제외.
                    let mut checked = !draft.contains(item.name);
                    if crate::checkbox(ui, th, &mut checked, item.name, true).changed() {
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
    hsep(ui, th);
    // 일괄 조작: 모두 선택(=빈 제외) / 모두 해제(=전체 제외).
    ui.horizontal(|ui| {
        if ghost_button(ui, th, labels.select_all).clicked() {
            draft.clear();
        }
        if ghost_button(ui, th, labels.deselect_all).clicked() {
            *draft = items.iter().map(|i| i.name.to_string()).collect();
        }
    });
    // 초기화(=전체 선택) / 적용.
    ui.horizontal(|ui| {
        if ghost_button(ui, th, labels.reset).clicked() {
            draft.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if primary_button(ui, th, labels.apply).clicked() {
                applied = true;
            }
        });
    });
    applied
}
