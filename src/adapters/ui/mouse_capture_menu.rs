//! 마우스 캡처 배너의 더보기 메뉴. 대상 surface는 dialogs에 보관한다.
//! 알림 끄기는 배너를 닫고, 캡처 비활성화는 배너를 남긴다.
//! 행은 공용 `banner_more_row` 다. 프로그램 이름만 줄이고, 고정 문구가 상한 폭을 넘으면 그 행만 줄을 바꾼다.
//! 프로그램 이름을 모르면(첫 foreground 조회 전, 조회 실패, 원격 mirror의 첫 값 수신 전·구버전 서버) 같은 메뉴를
//! 이름 미상 형태로 연다. mirror는 서버가 보낸 원격 foreground 이름을 쓴다.
//! 사유 줄과 비활성 두 행이며(공용 `banner_more_unknown`), 행을 눌러도 저장·닫기 없이 메뉴와 배너가 남는다.

use crate::adapters::ui::banner::BannerScope;
use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;
use crate::i18n::t;
use crate::intent::{OpenPopupMode, UiIntent};
use crate::state::MainViewState;
use crate::theme::{self, Theme};
use tasty_icons::Icon;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    BannerMoreLabel, BannerMoreUnknownCopy, banner_more_row, banner_more_row_height,
    banner_more_row_natural_width, banner_more_unknown, banner_more_unknown_height,
};

pub const MOUSE_CAPTURE_BANNER_MENU_POPUP_ID: crate::adapters::ui::popup::PopupId =
    "mouse_capture_banner_menu";

/// 셸 크기. 폭은 테두리를 뺀 안쪽 폭이다(셸 테두리는 바깥으로 그린다). 높이는 두 행과 위아래 패딩이다.
fn menu_size_for(th: &Theme, inner_width: LogicalPx, rows_height: f32) -> egui::Vec2 {
    let pad = th.banner_more_menu_padding().value();
    egui::vec2(inner_width.value(), pad * 2.0 + rows_height)
}

/// 시안의 메뉴 폭은 테두리까지 포함한 border-box다. 두 행 중 넓은 내용에 맞추고
/// `banner-more-menu-min-width`..`banner-more-menu-max-width` 로 제한한 뒤 테두리를 뺀 셸 폭을 돌려준다.
/// 상한에 걸리면 프로그램 이름만 줄이고, 고정 문구도 들어가지 않는 행은 줄을 바꾼다(공용 `banner_more_row`).
fn menu_inner_width(th: &Theme, widest_row: f32) -> LogicalPx {
    let pad = th.banner_more_menu_padding().value();
    let bw = th.border_width.value();
    let outer = (widest_row + pad * 2.0 + bw * 2.0).clamp(
        th.banner_more_menu_min_width().value(),
        th.banner_more_menu_max_width().value(),
    );
    LogicalPx(outer - bw * 2.0)
}

/// 셸 안쪽 폭에서 위아래 패딩과 같은 좌우 패딩을 뺀 행 폭.
fn row_width(th: &Theme, inner_width: LogicalPx) -> f32 {
    inner_width.value() - th.banner_more_menu_padding().value() * 2.0
}

/// 등록 시점의 크기. 대상이 정해지기 전이라 하한 폭과 한 줄 행 두 개를 쓴다.
pub fn menu_default_size() -> egui::Vec2 {
    let th = theme::theme();
    let min = th.banner_more_menu_min_width().value() - th.border_width.value() * 2.0;
    menu_size_for(&th, LogicalPx(min), th.menu_item_height().value() * 2.0)
}

/// `PopupDef.sizer` — 열 때 잰 크기를 쓴다. 매 프레임 호출되므로 글꼴 측정은 열 때 한 번만 한다.
pub fn menu_sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    match state.dialogs.mouse_capture_banner_menu_size {
        Some((width, height)) => egui::vec2(width.value(), height.value()),
        None => menu_default_size(),
    }
}

/// 두 행의 라벨. 언어마다 프로그램 이름의 위치가 달라 앞·뒤 문구를 따로 둔다.
fn row_labels(app_name: &str) -> [BannerMoreLabel<'_>; 2] {
    [
        BannerMoreLabel {
            prefix: t("popup.mouse_capture_banner_menu.suppress_prefix"),
            app: app_name,
            suffix: t("popup.mouse_capture_banner_menu.suppress_suffix"),
        },
        BannerMoreLabel {
            prefix: t("popup.mouse_capture_banner_menu.disable_prefix"),
            app: app_name,
            suffix: t("popup.mouse_capture_banner_menu.disable_suffix"),
        },
    ]
}

/// 이름 미상 형태의 문구. 이름 자리 없이 문장 전체를 쓴다.
fn unknown_copy() -> BannerMoreUnknownCopy<'static> {
    BannerMoreUnknownCopy {
        reason: t("popup.mouse_capture_banner_menu.unknown_reason"),
        rows: [
            t("popup.mouse_capture_banner_menu.notice_unknown"),
            t("popup.mouse_capture_banner_menu.capture_unknown"),
        ],
    }
}

/// 이름 미상 형태의 셸 크기. 줄을 바꾸는 문장을 담으므로 폭은 `banner-more-menu-max-width` 로 고정하고,
/// 높이는 그 폭에서 잰 사유 줄·구분선·두 행이다.
fn unknown_size(ctx: &egui::Context, th: &Theme) -> (LogicalPx, LogicalPx) {
    let width = th.banner_more_menu_max_width() - th.border_width * 2.0;
    let body = banner_more_unknown_height(ctx, th, unknown_copy(), LogicalPx(row_width(th, width)));
    (width, LogicalPx(menu_size_for(th, width, body.value()).y))
}

/// 메뉴 행이 저장할 foreground 프로그램 이름. trim 뒤 빈 이름은 모르는 것으로 본다.
pub(crate) fn known_app_name<'a>(
    engine: &'a crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
) -> Option<&'a str> {
    engine
        .foreground_name(surface_id)
        .filter(|name| !name.trim().is_empty())
}

/// 프로그램 이름에 맞춘 셸 크기. 폭은 넓은 행의 한 줄 폭을 상하한으로 묶고, 높이는 그 폭에서 잰 두 행이다.
fn measured_size(
    ctx: &egui::Context,
    th: &Theme,
    labels: &[BannerMoreLabel<'_>; 2],
) -> (LogicalPx, LogicalPx) {
    let widest = labels
        .iter()
        .map(|l| banner_more_row_natural_width(ctx, th, *l))
        .fold(0.0_f32, f32::max);
    let width = menu_inner_width(th, widest);
    let rows: f32 = labels
        .iter()
        .map(|l| banner_more_row_height(ctx, th, *l, row_width(th, width)))
        .sum();
    (width, LogicalPx(menu_size_for(th, width, rows).y))
}

/// 배너의 더보기 버튼에 맞춰 팝업을 연다. 아래 공간이 부족하면 위에 배치한다.
pub fn open(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    ctx: &egui::Context,
    scope: &BannerScope,
    trigger_rect: egui::Rect,
) {
    let BannerScope::Surface(surface_id) = scope else {
        return;
    };
    state.dialogs.mouse_capture_banner_menu_target = Some(*surface_id);

    let th = theme::theme();
    let app_name = known_app_name(engine, *surface_id);
    state.dialogs.mouse_capture_banner_menu_unknown = app_name.is_none();
    let (width, height) = match app_name {
        Some(name) => measured_size(ctx, &th, &row_labels(name)),
        None => unknown_size(ctx, &th),
    };
    state.dialogs.mouse_capture_banner_menu_size = Some((width, height));
    let size = egui::vec2(width.value(), height.value());
    // 셸 테두리는 바깥으로 그리므로 border-box 의 오른쪽 끝이 트리거 오른쪽에 맞도록 테두리만큼 당긴다.
    let bw = th.border_width.value();
    let offset = th.banner_more_menu_offset().value();
    let mut pos = egui::pos2(
        trigger_rect.right() - bw - size.x,
        trigger_rect.bottom() + offset + bw,
    );
    let screen = ctx.screen_rect();
    if pos.y + size.y + bw > screen.bottom() {
        pos.y = trigger_rect.top() - offset - bw - size.y;
    }

    state.dispatch_intent(
        UiIntent::OpenPopup {
            id: MOUSE_CAPTURE_BANNER_MENU_POPUP_ID,
            mode: OpenPopupMode::AtFocused(pos),
        }
        .from_user_menu("mouse_capture_banner"),
    );
}

/// `PopupDef.draw_fn` — 메뉴 콘텐츠만 그린다(셸은 headless popup 시스템이 그림).
pub fn draw_menu(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return PopupAction::Close;
    }
    let Some(surface_id) = state.dialogs.mouse_capture_banner_menu_target else {
        return PopupAction::Close;
    };
    let th = theme::theme();
    if state.dialogs.mouse_capture_banner_menu_unknown {
        // 비활성 행은 클릭을 받지 않는다. 저장·닫기 없이 메뉴를 유지하고 Esc·바깥 클릭으로만 닫는다.
        let _rows = draw_unknown_rows(ui, &th);
        return PopupAction::None;
    }
    let app_name = engine.foreground_name(surface_id).unwrap_or("").to_string();
    let (suppress_resp, disable_resp) = draw_menu_rows(ui, &th, row_labels(&app_name));

    if suppress_resp.clicked() {
        state.dispatch_intent(
            crate::intent::Intent::PatchSettings(
                crate::app::engine_action::SettingsPatch::SuppressMouseHint(app_name.clone()),
            )
            .from_user_context_menu(),
        );
        state.banners.close_shown_if_id(
            &BannerScope::Surface(surface_id),
            crate::adapters::ui::banner::defs::BANNER_MOUSE_CAPTURE,
        );
        return PopupAction::Close;
    }
    if disable_resp.clicked() {
        state.dispatch_intent(
            crate::intent::Intent::PatchSettings(
                crate::app::engine_action::SettingsPatch::DisableMouseCapture(app_name),
            )
            .from_user_context_menu(),
        );
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 두 행을 간격 없이 붙여 그린다. 셸 높이(`measured_size`)도 간격 없이 잰다.
fn draw_menu_rows(
    ui: &mut egui::Ui,
    th: &Theme,
    [suppress, disable]: [BannerMoreLabel<'_>; 2],
) -> (egui::Response, egui::Response) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let suppress = draw_menu_row(ui, th, icons::BELL, suppress);
    let disable = draw_menu_row(ui, th, icons::MOUSE, disable);
    (suppress, disable)
}

/// 이름 미상 형태 — 사유 줄, 구분선, 비활성 두 행(알림 bell, 캡처 mouse). 행 사이 간격은 없다.
fn draw_unknown_rows(ui: &mut egui::Ui, th: &Theme) -> [egui::Response; 2] {
    ui.spacing_mut().item_spacing.y = 0.0;
    let paint = |icon: Icon| {
        move |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
            icon.image(rect.height(), c).paint_at(ui, rect)
        }
    };
    let (info, bell, mouse) = (paint(icons::INFO), paint(icons::BELL), paint(icons::MOUSE));
    banner_more_unknown(ui, th, &info, [&bell, &mouse], unknown_copy())
}

fn draw_menu_row(
    ui: &mut egui::Ui,
    th: &Theme,
    icon: Icon,
    label: BannerMoreLabel<'_>,
) -> egui::Response {
    banner_more_row(
        ui,
        th,
        &|ui, rect, c| icon.image(rect.height(), c).paint_at(ui, rect),
        label,
        false,
        false,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 테두리까지 포함한 바깥 폭. 셸 폭에 바깥으로 그리는 테두리 두 줄을 더한다.
    fn outer(th: &Theme, widest: f32) -> f32 {
        menu_inner_width(th, widest).value() + th.border_width.value() * 2.0
    }

    /// 기본 item_spacing 이 0 이 아닌 Ui 에서도 두 행이 붙어 셸 높이의 행 부분과 같다.
    #[test]
    fn the_two_rows_sit_flush_at_the_menu_item_height() {
        let th = theme::theme();
        let ctx = egui::Context::default();
        let mut rows = None;
        let _frame = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(ui.spacing().item_spacing.y > 0.0);
                let (a, b) = draw_menu_rows(ui, &th, row_labels("vim"));
                rows = Some((a.rect, b.rect));
            });
        });
        let (a, b) = rows.expect("rows drawn");
        assert_eq!(b.top() - a.top(), th.menu_item_height().value());
        assert_eq!(b.bottom() - a.top(), th.menu_item_height().value() * 2.0);
    }

    #[test]
    fn a_short_row_takes_the_min_width() {
        let th = theme::theme();
        assert_eq!(outer(&th, 0.0), th.banner_more_menu_min_width().value());
    }

    #[test]
    fn a_long_row_stops_at_the_max_width() {
        let th = theme::theme();
        let past_max = th.banner_more_menu_max_width().value() * 2.0;
        assert_eq!(
            outer(&th, past_max),
            th.banner_more_menu_max_width().value()
        );
    }

    #[test]
    fn a_row_between_the_bounds_adds_the_padding_and_border() {
        let th = theme::theme();
        let min = th.banner_more_menu_min_width().value();
        let max = th.banner_more_menu_max_width().value();
        let widest = (min + max) * 0.5;
        let expected =
            widest + th.banner_more_menu_padding().value() * 2.0 + th.border_width.value() * 2.0;
        assert!(expected < max, "the sample row must stay under the cap");
        assert_eq!(outer(&th, widest), expected);
    }

    #[test]
    fn the_shell_keeps_the_width_and_fits_two_rows_and_the_padding() {
        let th = theme::theme();
        let width =
            LogicalPx(th.banner_more_menu_max_width().value() - th.border_width.value() * 2.0);
        let size = menu_size_for(&th, width, th.menu_item_height().value() * 2.0);
        assert_eq!(size.x, width.value());
        assert_eq!(
            size.y,
            th.banner_more_menu_padding().value() * 2.0 + th.menu_item_height().value() * 2.0
        );
    }

    /// `surface_id` 메뉴를 그리고 `row` 행을 실제 포인터 누름·뗌으로 클릭한다. 나온 요청을 돌려준다.
    /// 행은 한 줄 높이로 붙어 있으므로 `row` 번째 행의 가운데를 누른다.
    fn click_menu_row(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        row: usize,
    ) -> Vec<crate::intent::DispatchedIntent> {
        let th = theme::theme();
        state.dialogs.mouse_capture_banner_menu_target = Some(surface_id);
        let width = th.banner_more_menu_max_width().value();
        let h = th.menu_item_height().value();
        let at = egui::pos2(width * 0.5, h * (row as f32 + 0.5));
        let ctx = egui::Context::default();
        // 새 Area 는 첫 프레임에 크기만 재고 그리지 않으므로 빈 프레임을 먼저 둔다.
        let frames = [
            vec![],
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ];
        for events in frames {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _frame = ctx.run(input, |ctx| {
                egui::Area::new(egui::Id::new("mouse_capture_menu_test"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_width(width);
                        ui.set_max_width(width);
                        let _action = draw_menu(ui, state, engine);
                    });
            });
        }
        state.take_pending_intents()
    }

    /// 요청 중 설정 패치를 엔진 설정에 적용한다. 본체는 같은 `apply` 로 새 설정을 만든다.
    fn apply_patches(
        intents: &[crate::intent::DispatchedIntent],
        settings: &mut crate::settings::Settings,
    ) -> usize {
        let mut applied = 0;
        for intent in intents {
            if let crate::intent::Intent::PatchSettings(patch) = &intent.body {
                patch.apply(settings);
                applied += 1;
            }
        }
        applied
    }

    /// 대조군: 이름을 아는 surface 에서는 같은 클릭이 그 이름을 두 목록에 저장한다.
    /// 아래 이름 없음 시험의 클릭이 실제로 행에 닿는다는 근거다.
    #[test]
    fn a_known_foreground_name_is_saved_by_each_menu_row() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        e.live.foreground_names.insert(42, "vimtest".into());
        for row in 0..2 {
            let intents = click_menu_row(&mut state, &e.read(), 42, row);
            assert_eq!(apply_patches(&intents, &mut e.runtime.settings), 1);
        }
        let general = &e.runtime.settings.general;
        assert!(
            general
                .mouse_capture_banner_blacklist
                .contains(&"vimtest".to_string())
        );
        assert!(
            general
                .mouse_capture_blacklist
                .contains(&"vimtest".to_string())
        );
    }

    /// 메뉴를 실제처럼 연다. `open` 이 이름으로 형태(정상·이름 미상)와 크기를 정한다.
    fn open_menu(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
    ) {
        let ctx = egui::Context::default();
        let trigger = egui::Rect::from_min_size(egui::pos2(600.0, 40.0), egui::vec2(24.0, 24.0));
        let _frame = ctx.run(egui::RawInput::default(), |ctx| {
            open(
                state,
                engine,
                ctx,
                &BannerScope::Surface(surface_id),
                trigger,
            );
        });
        let _open_intents = state.take_pending_intents();
    }

    /// 열린 메뉴(열 때 잰 크기)를 그리고 `events` 프레임을 차례로 보낸다. 나온 요청과 Close 여부를 돌려준다.
    fn run_menu(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        frames: Vec<Vec<egui::Event>>,
    ) -> (Vec<crate::intent::DispatchedIntent>, bool) {
        let (width, _) = state
            .dialogs
            .mouse_capture_banner_menu_size
            .expect("opened");
        let ctx = egui::Context::default();
        let mut closed = false;
        for events in std::iter::once(vec![]).chain(frames) {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _frame = ctx.run(input, |ctx| {
                egui::Area::new(egui::Id::new("mouse_capture_menu_test"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_width(width.value());
                        ui.set_max_width(width.value());
                        if matches!(draw_menu(ui, state, engine), PopupAction::Close) {
                            closed = true;
                        }
                    });
            });
        }
        (state.take_pending_intents(), closed)
    }

    /// 메뉴 높이 전체를 위에서 아래로 4px 간격으로 누르고 떼는 프레임들. 사유 줄·구분선·두 행을 모두 지난다.
    fn sweep_clicks(state: &crate::state::MainViewState) -> Vec<Vec<egui::Event>> {
        let (width, height) = state
            .dialogs
            .mouse_capture_banner_menu_size
            .expect("opened");
        let x = width.value() * 0.5;
        let mut frames = Vec::new();
        let mut y = 1.0;
        while y < height.value() {
            let at = egui::pos2(x, y);
            frames.push(vec![egui::Event::PointerMoved(at)]);
            frames.push(vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }]);
            frames.push(vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            y += 4.0;
        }
        frames
    }

    fn key(key: egui::Key) -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    }

    fn push_banner(state: &mut crate::state::MainViewState, surface_id: u32) {
        state
            .banners
            .push(crate::adapters::ui::banner::BannerState::persistent(
                crate::adapters::ui::banner::defs::BANNER_MOUSE_CAPTURE,
                BannerScope::Surface(surface_id),
            ));
    }

    fn banner_shown(state: &crate::state::MainViewState, surface_id: u32) -> bool {
        state.banners.shown_banners().any(|b| {
            b.id == crate::adapters::ui::banner::defs::BANNER_MOUSE_CAPTURE
                && b.scope == BannerScope::Surface(surface_id)
        })
    }

    /// 이름이 없거나(원격 mirror·첫 조회 전·조회 실패) 공백뿐이면 이름 미상 형태로 열고 폭은 288 이다.
    /// 이름을 알면 정상 형태다.
    #[test]
    fn open_picks_the_unknown_form_without_a_name_at_the_max_width() {
        let th = theme::theme();
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        e.live.foreground_names.insert(43, "vim".into());
        e.live.foreground_names.insert(44, "  ".into());
        for (sid, unknown) in [(42, true), (43, false), (44, true)] {
            open_menu(&mut state, &e.read(), sid);
            assert_eq!(state.dialogs.mouse_capture_banner_menu_target, Some(sid));
            assert_eq!(
                state.dialogs.mouse_capture_banner_menu_unknown, unknown,
                "{sid}"
            );
            if unknown {
                let (width, height) = state.dialogs.mouse_capture_banner_menu_size.expect("size");
                assert_eq!(
                    width.value() + th.border_width.value() * 2.0,
                    th.banner_more_menu_max_width().value()
                );
                // 사유 줄과 구분선이 두 행 위에 더해진다.
                assert!(
                    height.value()
                        > th.banner_more_menu_padding().value() * 2.0
                            + th.menu_item_height().value() * 2.0
                );
            }
        }
    }

    /// 첫 busy 폴링 전에는 foreground 이름이 없다(`foreground_names` 항목 없음).
    /// 이 상태에서 연 메뉴의 어디를 눌러도 어느 목록에도 항목이 생기지 않고, 메뉴와 배너가 그대로 남는다.
    #[test]
    fn unknown_form_clicks_save_nothing_and_keep_the_menu_and_the_banner() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        assert_eq!(e.foreground_name(42), None);
        push_banner(&mut state, 42);
        open_menu(&mut state, &e.read(), 42);
        let frames = sweep_clicks(&state);
        let (intents, closed) = run_menu(&mut state, &e.read(), frames);
        assert!(intents.is_empty(), "{} intents", intents.len());
        assert!(!closed, "the menu stays open");
        assert_eq!(state.dialogs.mouse_capture_banner_menu_target, Some(42));
        assert!(banner_shown(&state, 42), "the banner stays");
    }

    /// 대조군: 이름을 알면 같은 훑기가 행을 눌러 설정을 저장하고 메뉴를 닫는다.
    /// 위 시험의 클릭이 행 자리에 닿는다는 근거다.
    #[test]
    fn the_same_sweep_acts_when_the_name_is_known() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        e.live.foreground_names.insert(42, "vimtest".into());
        push_banner(&mut state, 42);
        open_menu(&mut state, &e.read(), 42);
        let frames = sweep_clicks(&state);
        let (intents, closed) = run_menu(&mut state, &e.read(), frames);
        assert!(!intents.is_empty());
        assert!(closed);
    }

    /// mirror는 서버가 보낸 원격 이름으로 같은 행을 활성화하고 그 이름을 저장한다.
    #[test]
    fn a_mirror_name_from_the_server_enables_the_rows() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut e = session.borrow_mut();
        e.set_mirror_surface_foreground(42, Some("vimtest".into()));
        assert_eq!(known_app_name(&e.read(), 42), Some("vimtest"));
        push_banner(&mut state, 42);
        open_menu(&mut state, &e.read(), 42);
        assert!(!state.dialogs.mouse_capture_banner_menu_unknown);
        let frames = sweep_clicks(&state);
        let (intents, closed) = run_menu(&mut state, &e.read(), frames);
        assert!(!intents.is_empty());
        assert!(closed);
    }

    /// 열린 동안 이름이 생겨도 형태를 바꾸지 않는다. 같은 메뉴는 계속 비활성이고, 다음 열기에 정상 행이다.
    #[test]
    fn a_name_arriving_while_open_waits_for_the_next_open() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        open_menu(&mut state, &e.read(), 42);
        e.live.foreground_names.insert(42, "vimtest".into());
        let frames = sweep_clicks(&state);
        let (intents, closed) = run_menu(&mut state, &e.read(), frames);
        assert!(intents.is_empty() && !closed);
        assert!(state.dialogs.mouse_capture_banner_menu_unknown);
        open_menu(&mut state, &e.read(), 42);
        assert!(!state.dialogs.mouse_capture_banner_menu_unknown);
    }

    /// 이름 미상 형태에서 화살표·Tab·Enter·Space 는 저장도 닫기도 하지 않고, Esc 만 평소처럼 닫는다.
    /// 비활성 행이 포커스를 받지 않는다는 근거는 위젯 시험 `the_disabled_rows_never_take_focus` 다.
    #[test]
    fn unknown_form_keys_save_nothing_and_escape_closes() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        push_banner(&mut state, 42);
        open_menu(&mut state, &e.read(), 42);
        let keys = [
            egui::Key::ArrowDown,
            egui::Key::Enter,
            egui::Key::ArrowDown,
            egui::Key::Space,
            egui::Key::ArrowUp,
            egui::Key::Tab,
            egui::Key::Enter,
        ];
        let (intents, closed) = run_menu(&mut state, &e.read(), keys.map(key).to_vec());
        assert!(intents.is_empty() && !closed);
        assert!(banner_shown(&state, 42));
        let (intents, closed) = run_menu(&mut state, &e.read(), vec![key(egui::Key::Escape)]);
        assert!(intents.is_empty());
        assert!(closed);
    }

    /// ja 처럼 고정 문구가 상한 폭을 넘는 행이 있으면 메뉴는 288 에 머물고 셸이 그 행만큼 높아진다.
    /// 그린 두 행의 높이 합이 열 때 잰 셸 높이와 같아야 셸이 행을 자르지 않는다.
    #[test]
    fn a_wrapped_row_keeps_the_max_width_and_the_shell_fits_the_drawn_rows() {
        let th = theme::theme();
        let labels = [
            BannerMoreLabel {
                prefix: "",
                app: "vim",
                suffix: "についてこのお知らせをオフにする",
            },
            BannerMoreLabel {
                prefix: "",
                app: "vim",
                suffix: "についてマウスキャプチャを常に無効にする",
            },
        ];
        let ctx = egui::Context::default();
        let mut measured = None;
        let mut drawn = None;
        for _ in 0..2 {
            let _frame = ctx.run(egui::RawInput::default(), |ctx| {
                let (width, height) = measured_size(ctx, &th, &labels);
                measured = Some((width, height));
                egui::Area::new(egui::Id::new("menu")).show(ctx, |ui| {
                    ui.set_width(row_width(&th, width));
                    ui.set_max_width(row_width(&th, width));
                    let (a, b) = draw_menu_rows(ui, &th, labels);
                    drawn = Some(b.rect.bottom() - a.rect.top());
                });
            });
        }
        let (width, height) = measured.expect("measured");
        assert_eq!(
            width.value() + th.border_width.value() * 2.0,
            th.banner_more_menu_max_width().value()
        );
        let pad = th.banner_more_menu_padding().value();
        let rows = height.value() - pad * 2.0;
        assert!(rows > th.menu_item_height().value() * 2.0, "a row wrapped");
        assert_eq!(drawn.expect("drawn"), rows);
    }
}
