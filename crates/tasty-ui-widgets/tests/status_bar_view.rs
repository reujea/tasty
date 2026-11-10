//! 상태바의 클릭·리사이즈 우선권·상대 좌표 배치·좁은 폭에서의 클릭 영역을 검사한다.
//! 축소 단계의 선택은 단위 검사가 담당하고 여기서는 실제 입력 결과를 확인한다.

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    StatusBarAction, StatusBarData, StatusBarDrawResult, draw_status_bar_view, kbd_width,
};

const BAR_W: f32 = 600.0;
// view 의 디자인 inline 레이아웃 값(`gallery/layouts/index.jsx` statusbar: 컨테이너
// `padding: "0 10px"` + `gap: 10`) — 셀 좌표를 테스트에서 독립적으로 재계산해
// 배치 계약을 고정한다.
const BAR_PAD_X: f32 = 10.0;
const ITEM_GAP: f32 = 10.0;

const PALETTE_KEYS: &str = "Ctrl+K";

fn data() -> StatusBarData {
    StatusBarData {
        remote: None,
        branch: Some("main".into()),
        surface_id: Some(3),
        pane_id: Some(1),
        shell: Some("zsh".into()),
        grid: Some((120, 32)),
        theme_is_light: false,
        palette_keys: PALETTE_KEYS.into(),
        palette_tooltip: "Open the command palette".into(),
        theme_tooltip: "Toggle theme".into(),
    }
}

fn raw(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
        focused: true,
        events,
        ..Default::default()
    }
}

/// 임의 위치의 부모 `Ui` — 본체는 `egui::Area`, 갤러리는 카드 안이라 둘 다 "주어진 Ui".
fn frame_w(
    ctx: &egui::Context,
    theme: &Theme,
    origin: Pos2,
    width: f32,
    d: &StatusBarData,
    events: Vec<Event>,
) -> StatusBarDrawResult {
    let mut out = StatusBarDrawResult::default();
    // FullOutput 불필요 — 이 테스트는 view 가 보고한 action/hover 만 확인한다.
    let _out = ctx.run(raw(events), |c| {
        egui::Area::new(egui::Id::new("host"))
            .fixed_pos(origin)
            .show(c, |ui| {
                out = draw_status_bar_view(ui, theme, LogicalPx(width), d);
            });
    });
    out
}

fn frame(
    ctx: &egui::Context,
    theme: &Theme,
    origin: Pos2,
    d: &StatusBarData,
    events: Vec<Event>,
) -> StatusBarDrawResult {
    frame_w(ctx, theme, origin, BAR_W, d, events)
}

/// 공용 키캡 폭과 바깥 여백으로 오른쪽 두 버튼의 클릭 좌표를 계산한다.
fn right_cluster_centers(
    ctx: &egui::Context,
    theme: &Theme,
    origin: Pos2,
    width: f32,
) -> (f32, f32) {
    let theme_w = theme.statusbar_glyph_size().value();
    let palette_w = kbd_width(ctx, theme, PALETTE_KEYS).value();
    let right = origin.x + width - BAR_PAD_X;
    (
        right - theme_w - ITEM_GAP - palette_w / 2.0,
        right - theme_w / 2.0,
    )
}

fn ptr_move(p: Pos2) -> Event {
    Event::PointerMoved(p)
}

fn ptr_btn(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    }
}

/// hover 이동 → press → release 3 프레임. `clicked()` 는 release 프레임에 뜬다.
fn click_at_w(
    ctx: &egui::Context,
    theme: &Theme,
    origin: Pos2,
    width: f32,
    p: Pos2,
) -> StatusBarDrawResult {
    let d = data();
    frame_w(ctx, theme, origin, width, &d, vec![ptr_move(p)]);
    frame_w(ctx, theme, origin, width, &d, vec![ptr_btn(p, true)]);
    frame_w(ctx, theme, origin, width, &d, vec![ptr_btn(p, false)])
}

fn click_at(ctx: &egui::Context, theme: &Theme, origin: Pos2, p: Pos2) -> StatusBarDrawResult {
    click_at_w(ctx, theme, origin, BAR_W, p)
}

fn hover_at(ctx: &egui::Context, theme: &Theme, origin: Pos2, p: Pos2) -> StatusBarDrawResult {
    let d = data();
    frame(ctx, theme, origin, &d, vec![ptr_move(p)]);
    frame(ctx, theme, origin, &d, vec![ptr_move(p)])
}

/// 배치 계산에 쓰는 폰트 metric 을 얻으려면 프레임을 한 번 돌려 font 를 초기화해야 한다.
fn warmed_ctx(theme: &Theme) -> egui::Context {
    let ctx = egui::Context::default();
    frame(&ctx, theme, Pos2::ZERO, &data(), vec![]);
    ctx
}

#[test]
fn palette_and_theme_cells_report_their_actions() {
    let theme = tasty_themes::mocha_fallback();
    let ctx = warmed_ctx(&theme);
    let origin = Pos2::ZERO;
    let (palette_x, theme_x) = right_cluster_centers(&ctx, &theme, origin, BAR_W);
    let y = origin.y + 12.0;

    let out = click_at(&ctx, &theme, origin, pos2(theme_x, y));
    assert_eq!(out.actions, vec![StatusBarAction::ToggleTheme]);

    let out = click_at(&ctx, &theme, origin, pos2(palette_x, y));
    assert_eq!(out.actions, vec![StatusBarAction::OpenPalette]);

    // 좌측 클러스터(브랜치 셀)는 비클릭 — 어떤 액션도 나오지 않는다.
    let out = click_at(&ctx, &theme, origin, pos2(origin.x + 15.0, y));
    assert!(out.actions.is_empty(), "좌측 클러스터는 표시 전용이다");
}

#[test]
fn hover_over_clickable_cells_sets_resize_priority() {
    let theme = tasty_themes::mocha_fallback();
    let ctx = warmed_ctx(&theme);
    let origin = Pos2::ZERO;
    let (palette_x, theme_x) = right_cluster_centers(&ctx, &theme, origin, BAR_W);
    let y = origin.y + 12.0;

    assert!(hover_at(&ctx, &theme, origin, pos2(theme_x, y)).resize_priority_hovered);
    assert!(hover_at(&ctx, &theme, origin, pos2(palette_x, y)).resize_priority_hovered);
    assert!(
        !hover_at(&ctx, &theme, origin, pos2(origin.x + 15.0, y)).resize_priority_hovered,
        "비클릭 좌측 클러스터는 리사이즈 우선권을 뺏지 않는다"
    );
}

/// 부모 `Ui` 가 화면 원점이 아니어도 같은 상대 좌표에서 같은 셀이 잡혀야 한다.
#[test]
fn layout_is_relative_to_the_parent_ui_not_the_screen_origin() {
    let theme = tasty_themes::mocha_fallback();
    let ctx = warmed_ctx(&theme);
    let origin = pos2(137.0, 211.0);
    let (palette_x, theme_x) = right_cluster_centers(&ctx, &theme, origin, BAR_W);
    let y = origin.y + 12.0;

    assert_eq!(
        click_at(&ctx, &theme, origin, pos2(theme_x, y)).actions,
        vec![StatusBarAction::ToggleTheme],
        "절대 좌표 잔재가 있으면 오프셋된 부모에서 셀이 어긋난다"
    );
    assert_eq!(
        click_at(&ctx, &theme, origin, pos2(palette_x, y)).actions,
        vec![StatusBarAction::OpenPalette]
    );
    assert!(hover_at(&ctx, &theme, origin, pos2(theme_x, y)).resize_priority_hovered);
    assert!(
        !hover_at(&ctx, &theme, origin, pos2(5.0, 5.0)).resize_priority_hovered,
        "바가 옮겨갔는데 화면 원점에서 hover 가 잡히면 절대 좌표를 쓰고 있는 것이다"
    );
}

/// 좁아지면 팔레트 키캡이 먼저 빠지고, 테마 글리프는 끝까지 남는다.
#[test]
fn the_palette_cap_drops_before_the_theme_glyph() {
    let theme = tasty_themes::mocha_fallback();
    let ctx = warmed_ctx(&theme);
    let origin = Pos2::ZERO;
    let y = origin.y + 12.0;

    let narrow = BAR_PAD_X * 2.0 + theme.icon_glyph_size_xs.value() * 2.0 + ITEM_GAP + 1.0;
    let (palette_x, theme_x) = right_cluster_centers(&ctx, &theme, origin, narrow);

    assert_eq!(
        click_at_w(&ctx, &theme, origin, narrow, pos2(theme_x, y)).actions,
        vec![StatusBarAction::ToggleTheme],
        "테마 글리프는 어느 단계에서도 빠지지 않는다"
    );
    assert!(
        click_at_w(&ctx, &theme, origin, narrow, pos2(palette_x, y))
            .actions
            .is_empty(),
        "키캡이 접혔는데 그 자리가 여전히 클릭을 먹는다"
    );
}
