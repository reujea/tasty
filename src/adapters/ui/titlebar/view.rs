//! 앱·OS 상태에 직접 접근하지 않는 타이틀바 렌더링. 클릭·드래그 결과는 호출부에서 처리한다.

use crate::theme::Theme;

/// Linux에서 그릴 창 버튼. macOS는 네이티브 버튼, Windows는 caption.rs를 사용한다.
// 이유: Linux DE 버튼만 이 enum 을 구성한다 — macOS 신호등·Windows 캡션 빌드엔 생성처가 없다.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// 컨트롤 버튼 클러스터의 측면. Linux DE 관습(GNOME/KDE=우측, 일부 좌측).
// 이유: `WindowButton` 과 같다 — 이 측면 값을 만드는 곳이 Linux DE 프리셋뿐이다.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlSide {
    // 이유: 현재 os_controls는 오른쪽 배치만 사용하며 왼쪽 배치는 생성하지 않는다.
    #[allow(dead_code)]
    Left,
    Right,
}

/// tasty 가 그리는 DE 가변 버튼 묶음 (집합·순서·측면). 디자인 `titlebar_linux.jsx`
/// 의 `buttons`/`side` props 에 대응. 비어 있으면 버튼을 그리지 않는다.
#[derive(Debug, Clone)]
pub struct TitlebarControls {
    /// 좌→우(또는 측면 기준) 순서대로 그릴 버튼.
    pub buttons: Vec<WindowButton>,
    /// 클러스터를 titlebar 의 어느 쪽에 붙일지.
    pub side: ControlSide,
}

/// 타이틀바 화면 입력.
pub struct TitlebarProps<'a> {
    pub theme: &'a Theme,
    /// 윈도우 포커스 여부 — active/inactive 디밍 결정.
    pub active: bool,
    /// titlebar 높이 (logical points = egui 좌표). theme `titlebar_height` 토큰.
    pub height: f32,
    /// 좌측 컨트롤 슬롯 폭 (logical points). macOS 네이티브 신호등
    /// (standardWindowButton)이 좌상단에 OS 렌더되는 영역 — 이 폭만큼은 드래그 hit 를
    /// 두지 않아 신호등 클릭이 드래그로 새지 않게 한다. 신호등 없는 OS 에서는 0.
    pub left_inset: f32,
    /// tasty 가 직접 그리는 윈도우 컨트롤 버튼 (Linux DE 가변). `None` 이면 그리지
    /// 않는다(macOS 네이티브 신호등; Windows 는 전용 caption.rs 로 그림).
    pub controls: Option<TitlebarControls>,
    /// 윈도우 maximize 상태 — Windows 캡션 버튼의 maximize↔restore 글리프 토글에
    /// 사용한다. 캡션 버튼이 없는 OS 에서는 무시된다.
    // 이유: 이 필드를 읽는 곳이 Windows 캡션 버튼의 글리프 토글뿐이다(위).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub maximized: bool,
    /// 창 버튼이 키보드 포커스 표시를 갖는 창이다(Linux 설정창). 참이면 포커스를 받은 버튼에 포커스 링을 그리고,
    /// 이름 툴팁을 디자인 시스템 Tooltip으로 버튼 아래 오른쪽 끝 맞춤에 hover와 포커스 때 띄운다.
    /// 메인 창은 키 입력이 터미널로 가서 버튼이 포커스를 받지 않으므로 거짓이다.
    // 이유: 참으로 넘기는 곳이 Linux 설정창뿐이고, 이 값을 읽는 Linux DE 버튼도 Linux 빌드에만 그려진다.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub keyboard_focus: bool,
    /// 빈 영역 드래그 가운데 주 버튼(왼쪽) 드래그만 창 이동으로 보고한다(Linux 설정창, 디자인 정본과 같다).
    /// 거짓이면 어느 버튼의 드래그든 이동이다(메인 창).
    pub primary_drag_only: bool,
    /// 이번 패스의 키 주인이 팝업이다(Linux 설정창). 참이면 창 버튼이 Tab 포커스를 받지 않아, 팝업이 닫힌 뒤
    /// Space·Enter가 그 포커스로 버튼을 누르지 않는다.
    // 이유: 참으로 넘기는 곳이 Linux 설정창뿐이고, 이 값을 읽는 Linux DE 버튼도 Linux 빌드에만 그려진다.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub popup_owns_keys: bool,
}

/// titlebar view 가 보고하는 사용자 의도. wrapper 가 winit window 조작 / app 이벤트로 변환.
///
/// `Minimize`/`Close` 는 Linux DE 버튼(`draw_window_buttons`)·Windows 캡션
/// (`caption.rs`)에서 생성된다. macOS 는 네이티브 신호등이라 StartDrag/ToggleMaximize
/// 만 생성하므로 그 빌드에선 미사용 variant 의 dead_code 를 허용한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// 이유: macOS 는 네이티브 신호등이라 `Minimize`/`Close` 생성처가 그 빌드에 없다(위).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub enum TitlebarAction {
    /// 비인터랙티브(드래그) 영역에서 드래그 시작 → 윈도우 이동.
    StartDrag,
    /// 드래그 영역 더블클릭 또는 maximize 버튼 → maximize 토글.
    ToggleMaximize,
    /// minimize 버튼 → 윈도우 최소화.
    Minimize,
    /// close 버튼 → 윈도우 닫기(quit/close 라이프사이클 라우팅).
    Close,
}

/// [`draw_titlebar_view`] 의 반환값 — 사용자 액션 + 가장자리 리사이즈 우선권 판정.
pub struct TitlebarDrawResult {
    /// wrapper 가 winit window 조작/app 이벤트로 변환할 사용자 의도.
    pub actions: Vec<TitlebarAction>,
    /// 버튼 위에서만 가장자리 리사이즈보다 버튼 조작을 우선한다. 빈 여백은 제외한다.
    pub resize_priority_hovered: bool,
    /// 이번 프레임의 닫기 버튼 영역. 저장이 멈춘 동안에도 이 버튼은 포인터를 받는다.
    pub close_rect: Option<egui::Rect>,
    /// 이번 프레임 타이틀바 패널의 배치 영역(논리 좌표). 아래 변은 작업 영역의 위 변이다.
    pub panel_rect: egui::Rect,
}

/// 제목 줄 빈 영역(창 이동)의 egui id 이름. 창 가장자리 판정이 이 영역을 위젯으로 보지 않게 한다.
pub const DRAG_AREA_ID: &str = "tasty_titlebar_drag";

/// 상단 바와 창 버튼을 그린다. 버튼·macOS 네이티브 버튼 영역은 드래그에서 제외한다.
pub fn draw_titlebar_view(ctx: &egui::Context, props: &TitlebarProps) -> TitlebarDrawResult {
    let th = props.theme;
    let mut actions = Vec::new();
    let mut resize_priority_hovered = false;
    let mut close_rect = None;

    let bg = if props.active {
        th.titlebar_bg()
    } else {
        th.titlebar_bg_inactive()
    };

    let panel = egui::TopBottomPanel::top("tasty_titlebar")
        .exact_height(props.height)
        .frame(egui::Frame::new().fill(bg.to_egui()))
        .show_separator_line(false)
        .show(ctx, |ui| {
            let rect = ui.max_rect();

            let mut left_controls_w = 0.0_f32;
            let mut right_controls_w = 0.0_f32;
            if let Some(controls) = &props.controls
                && !controls.buttons.is_empty()
            {
                let strip = draw_window_buttons(
                    ui,
                    rect,
                    props,
                    controls,
                    &mut actions,
                    &mut resize_priority_hovered,
                    &mut close_rect,
                );
                match controls.side {
                    ControlSide::Left => left_controls_w = strip,
                    ControlSide::Right => right_controls_w = strip,
                }
            }

            // Windows는 별도 캡션 버튼을 오른쪽에 배치한다.
            #[cfg(target_os = "windows")]
            {
                let caption_w = super::caption::cluster_width(th);
                let caption_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.right() - caption_w, rect.top()),
                    rect.max,
                );
                let caption_result = super::caption::draw_caption_buttons(ui, caption_rect, props);
                actions.extend(caption_result.actions);
                resize_priority_hovered |= caption_result.hovered;
                close_rect = close_rect.or(caption_result.close_rect);
                right_controls_w = right_controls_w.max(caption_w);
            }

            // 창 버튼 영역을 뺀 나머지에서만 드래그를 시작한다.
            let drag_left = rect.left() + props.left_inset + left_controls_w;
            let drag_right = rect.right() - right_controls_w;
            if drag_right > drag_left {
                let drag_rect = egui::Rect::from_min_max(
                    egui::pos2(drag_left, rect.top()),
                    egui::pos2(drag_right, rect.bottom()),
                );
                let resp = ui.interact(
                    drag_rect,
                    egui::Id::new(DRAG_AREA_ID),
                    egui::Sense::click_and_drag(),
                );
                let drag_started = if props.primary_drag_only {
                    // 누름은 드래그가 시작되기 전 패스에 오므로 패스마다 적는다.
                    let primary_press = drag_began_with_primary(ui, resp.id, drag_rect);
                    resp.drag_started_by(egui::PointerButton::Primary) && primary_press
                } else {
                    resp.drag_started()
                };
                if resp.double_clicked() {
                    actions.push(TitlebarAction::ToggleMaximize);
                } else if drag_started {
                    actions.push(TitlebarAction::StartDrag);
                }
            }

            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                egui::Stroke::new(
                    th.border_width.value(),
                    th.titlebar_border().to_egui_premultiplied(),
                ),
            );
        });

    TitlebarDrawResult {
        actions,
        resize_priority_hovered,
        close_rect,
        panel_rect: panel.response.rect,
    }
}

/// 이 드래그 영역(`drag_rect`)을 egui 드래그 후보로 만든 누름이 왼쪽 버튼인지. egui의 `drag_started_by`는 지금 그
/// 버튼이 눌려 있는지만 보므로, 후보를 만든 누름의 버튼을 `id`의 임시 값에 따로 적는다.
///
/// egui 0.31(`interaction.rs`)은 후보가 빈 동안의 누름이 드래그 위젯을 맞히면 그 위젯을 후보로 정하고, 패스 첫머리의
/// Escape, 어느 버튼의 뗌, 포인터가 창을 떠남으로 후보를 비운다. 같은 순서로, 기록이 빈 동안 이 영역 위의 누름만 그
/// 버튼을 적고 Escape·뗌·`PointerGone`은 기록을 지운다. 영역 밖 누름(본문·닫기 버튼)은 적지 않는다. 다른 드래그 위젯이 후보인 동안 이 영역 위 누름을
/// 적더라도, 그 후보가 풀리는 뗌·Escape가 기록도 지우고 그동안 이 영역의 드래그는 시작되지 않는다.
fn drag_began_with_primary(ui: &egui::Ui, id: egui::Id, drag_rect: egui::Rect) -> bool {
    let key = id.with("drag_press_primary");
    let mut armed: Option<bool> = ui.data(|d| d.get_temp(key)).flatten();
    ui.input(|i| {
        if i.key_pressed(egui::Key::Escape) {
            armed = None;
        }
        for event in &i.events {
            match event {
                egui::Event::PointerButton { pressed: false, .. } | egui::Event::PointerGone => {
                    armed = None
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    ..
                } if armed.is_none() && drag_rect.contains(*pos) => {
                    armed = Some(*button == egui::PointerButton::Primary);
                }
                _ => {}
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(key, armed));
    armed == Some(true)
}

/// DE 가변 버튼 클러스터를 측면에 그리고, 차지한 strip 폭(logical px)을 반환한다.
/// 클릭은 `actions` 로, 버튼 hover 는 `hovered` 로 보고한다(OR 누적 — 호출부가
/// 매 프레임 `false` 로 초기화한 값을 넘긴다).
fn draw_window_buttons(
    ui: &egui::Ui,
    rect: egui::Rect,
    props: &TitlebarProps,
    controls: &TitlebarControls,
    actions: &mut Vec<TitlebarAction>,
    hovered: &mut bool,
    close_rect: &mut Option<egui::Rect>,
) -> f32 {
    let th = props.theme;
    let d = th.window_button_size.value(); // 원형 버튼 지름
    let edge_pad = th.spacing_sm.value(); // 측면 끝 여백
    let gap = th.spacing_xs.value(); // 버튼 간 간격
    let n = controls.buttons.len();
    let strip_w = edge_pad * 2.0 + d * n as f32 + gap * (n.saturating_sub(1)) as f32;
    let cy = rect.center().y;

    let (mut cx, step) = match controls.side {
        ControlSide::Right => (rect.right() - edge_pad - d * 0.5, -(d + gap)),
        ControlSide::Left => (rect.left() + edge_pad + d * 0.5, d + gap),
    };

    // 오른쪽 배치에서는 마지막 버튼이 바깥쪽에 오도록 역순으로 그린다.
    let order: Vec<&WindowButton> = match controls.side {
        ControlSide::Right => controls.buttons.iter().rev().collect(),
        ControlSide::Left => controls.buttons.iter().collect(),
    };

    for (i, &&button) in order.iter().enumerate() {
        let center = egui::pos2(cx, cy);
        let btn_rect = egui::Rect::from_center_size(center, egui::vec2(d, d));
        let id = egui::Id::new(("tasty_titlebar_btn", i, button as u8 as usize));
        let label = match button {
            WindowButton::Minimize => crate::i18n::t("titlebar.minimize"),
            WindowButton::Maximize => crate::i18n::t("titlebar.maximize"),
            WindowButton::Close => crate::i18n::t("titlebar.close"),
        };
        let sense = if props.popup_owns_keys {
            egui::Sense::CLICK
        } else {
            egui::Sense::click()
        };
        let resp = ui.interact(btn_rect, id, sense);
        let resp = if props.keyboard_focus {
            let shown = tasty_ui_widgets::tooltip_hover_delay_elapsed(
                ui.ctx(),
                th,
                resp.id,
                resp.hovered(),
            ) || resp.has_focus();
            if shown {
                tasty_ui_widgets::Tooltip::new(label)
                    .id_source(("tasty_titlebar_btn_tooltip", resp.id))
                    .below_end(ui.ctx(), th, btn_rect)
                    .show(ui, th, btn_rect);
            }
            resp
        } else {
            resp.on_hover_text(label)
        };
        *hovered |= resp.hovered();

        let is_close = matches!(button, WindowButton::Close);
        if is_close {
            *close_rect = Some(btn_rect);
        }
        let bg = if resp.is_pointer_button_down_on() {
            if is_close {
                Some(th.accent_window_close().to_egui())
            } else {
                Some(th.active_overlay.to_egui_premultiplied())
            }
        } else if resp.hovered() {
            if is_close {
                Some(th.accent_window_close().to_egui())
            } else {
                Some(th.hover_overlay.to_egui_premultiplied())
            }
        } else {
            None
        };
        if let Some(bg) = bg {
            ui.painter().circle_filled(center, d * 0.5, bg);
        }

        let fg = if is_close && resp.hovered() {
            th.text_on_window_close()
        } else if props.active {
            th.titlebar_fg()
        } else {
            th.titlebar_fg_inactive()
        };
        let stroke = egui::Stroke::new(th.border_width.value(), fg.to_egui());
        draw_button_glyph(
            ui.painter(),
            center,
            th.titlebar_window_glyph_size().value() * 0.5,
            &button,
            stroke,
        );
        if props.keyboard_focus && resp.has_focus() {
            // 버튼 둘레에서 focus-ring-offset만큼 띄운 focus-ring-width 링. 띄움은 버튼처럼 UI 배율 밖이고 링 굵기
            // (focus_ring_width)는 배율을 받는다.
            // epaint 원 획은 반지름 바깥으로 그리므로 반지름은 링의 안쪽 변이다.
            let ring = th.focus_ring_width.value();
            let offset = th.focus_ring_offset.value();
            ui.painter().circle_stroke(
                center,
                d * 0.5 + offset,
                egui::Stroke::new(ring, th.border_focus().to_egui()),
            );
        }

        if resp.clicked() {
            actions.push(match button {
                WindowButton::Minimize => TitlebarAction::Minimize,
                WindowButton::Maximize => TitlebarAction::ToggleMaximize,
                WindowButton::Close => TitlebarAction::Close,
            });
        }

        cx += step;
    }

    strip_w
}

/// 버튼 글리프(min=가로선, max=사각형, close=×)를 중심 기준으로 그린다. `g`는 중심에서 각 변까지의 거리로
/// titlebar-window-glyph-size의 절반이다.
fn draw_button_glyph(
    painter: &egui::Painter,
    center: egui::Pos2,
    g: f32,
    button: &WindowButton,
    stroke: egui::Stroke,
) {
    match button {
        WindowButton::Minimize => {
            painter.line_segment(
                [
                    egui::pos2(center.x - g, center.y),
                    egui::pos2(center.x + g, center.y),
                ],
                stroke,
            );
        }
        WindowButton::Maximize => {
            let r = egui::Rect::from_center_size(center, egui::vec2(g * 2.0, g * 2.0));
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
        }
        WindowButton::Close => {
            painter.line_segment(
                [
                    egui::pos2(center.x - g, center.y - g),
                    egui::pos2(center.x + g, center.y + g),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - g, center.y + g),
                    egui::pos2(center.x + g, center.y - g),
                ],
                stroke,
            );
        }
    }
}
