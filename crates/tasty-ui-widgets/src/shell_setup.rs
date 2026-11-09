//! 첫 실행 셸 설정 화면. 본체와 갤러리가 함께 호출한다.
//!
//! 부팅 화면과 같은 `bg-app` 채움 위에 로고 락업과 `size-360` 폭 폼을 세로 가운데에 쌓는다.
//! 폼은 제목, 부제, (Windows) Git Bash 안내 줄, mono 경로 입력, 검증 줄, 오른쪽 정렬 버튼 줄이며
//! 항목 사이는 `space-sm`이다. 카드는 없다. 검증 줄은 문구가 없어도 caption 한 줄 높이를 잡는다.
//!
//! 이 view는 경로를 판정하지 않는다. 판정 결과와 문구는 호출부가 넘기고 눌린 동작만 돌려준다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::brand::draw_wordmark;
use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::input::Input;
use crate::tokens::STRUCT_GAP_1;

/// 폼 폭. 디자인 `ShellSetupFrame`의 `--tasty-size-360`이며 역할 토큰이 없다.
pub const SHELL_SETUP_FORM_WIDTH: LogicalPx = LogicalPx(360.0);

/// 호스트가 경로를 판정한 결과. 검증 줄은 판정마다 한 문구를 보인다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellSetupCheck {
    /// 경로가 비었다. 검증 줄은 비우고 높이만 잡는다.
    Empty,
    /// 경로에 파일이 없다.
    Missing,
    /// Windows 전용. 경로에 무언가 있지만 Git Bash가 아니다.
    NotShell,
    /// macOS·Linux 전용. 경로가 디렉터리다.
    Directory,
    /// macOS·Linux 전용. 파일에 실행 권한이 없다.
    NotExecutable,
    /// 쓸 수 있는 셸이다.
    Valid,
}

/// 화면 문구. i18n은 호출부가 한다.
pub struct ShellSetupView<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    /// Windows에서만 넘긴다. 부제와 입력 사이에 경고 caption 줄로 그린다.
    pub git_bash_notice: Option<&'a str>,
    pub placeholder: &'a str,
    pub missing: &'a str,
    pub not_shell: &'a str,
    pub directory: &'a str,
    pub not_executable: &'a str,
    pub valid: &'a str,
    pub quit: &'a str,
    pub confirm: &'a str,
    pub check: ShellSetupCheck,
}

/// 이번 프레임에 눌린 동작.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellSetupOutput {
    /// 확인 버튼 또는 입력에서 Enter. 판정이 `Valid`일 때만 참이다.
    pub confirm: bool,
    /// Quit 버튼. 앱을 끝낸다.
    pub quit: bool,
}

/// `ui`의 가용 영역 전체를 `bg-app`으로 채우고 락업과 폼을 가운데에 그린다.
pub fn shell_setup_screen(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &ShellSetupView<'_>,
    path: &mut String,
) -> ShellSetupOutput {
    let area = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(area, 0.0, theme.bg_app().to_egui());

    // 문구 줄바꿈에 따라 높이가 바뀌므로 지난 패스에서 잰 높이로 세로 가운데를 잡는다.
    let height_id = ui.id().with("shell_setup_content_height");
    let known = ui.data(|d| d.get_temp::<f32>(height_id)).unwrap_or(0.0);
    let top_pad = ((area.height() - known) / 2.0).max(0.0);

    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    content.spacing_mut().item_spacing = egui::Vec2::ZERO;
    content.add_space(top_pad);
    let top = content.cursor().top();
    draw_wordmark(
        &mut content,
        theme,
        theme.loading_screen_wordmark_icon_size(),
        theme.loading_screen_wordmark_font_size(),
        theme.loading_lockup_tracking(),
    );
    content.add_space(theme.spacing_xl.value());
    let width = SHELL_SETUP_FORM_WIDTH.value().min(area.width());
    let output = content
        .allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                draw_form(ui, theme, view, path)
            },
        )
        .inner;
    let measured = content.cursor().top() - top;
    if (measured - known).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(height_id, measured));
        ui.ctx()
            .request_discard("shell setup content height changed");
    }
    ui.allocate_rect(area, egui::Sense::hover());
    output
}

fn draw_form(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &ShellSetupView<'_>,
    path: &mut String,
) -> ShellSetupOutput {
    let gap = theme.spacing_sm.value();
    ui.spacing_mut().item_spacing = egui::vec2(0.0, gap);
    let line_ui = theme.line_height_ui;

    // 600 굵기는 굵은 UI 글꼴이 없어 크기와 색으로 근사한다(디자인 정합 지침 §타이포그래피).
    ui.label(
        egui::RichText::new(view.title)
            .size(theme.font_size_max.value())
            .color(theme.text_primary().to_egui()),
    );
    let body = theme.font_size_body.value();
    ui.add(
        egui::Label::new(
            egui::RichText::new(view.subtitle)
                .size(body)
                .line_height(Some(body * line_ui))
                .color(theme.text_muted().to_egui()),
        )
        .wrap(),
    );
    if let Some(notice) = view.git_bash_notice {
        notice_line(ui, theme, notice);
    }

    let response = Input::new()
        .mono(true)
        .placeholder(view.placeholder)
        .show(ui, theme, path);
    check_line(ui, theme, view);

    let valid = view.check == ShellSetupCheck::Valid;
    let mut output = ShellSetupOutput::default();
    // 시안의 버튼 줄은 폼 gap 위에 `space-sm`을 한 번 더 둔다.
    ui.add_space(gap);
    let row_h = ControlSize::Md.height(theme);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_h),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let confirm = Button::new(view.confirm)
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Md)
                .enabled(valid)
                .show(ui, theme);
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            output.confirm = valid && (confirm.clicked() || enter);
            output.quit = Button::new(view.quit)
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Md)
                .show(ui, theme)
                .clicked();
        },
    );
    output
}

/// Windows 전용 Git Bash 안내. 상자 없이 경고색 caption 줄이며 글리프는 첫 줄 위쪽에 붙는다.
fn notice_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let warning = theme.accent_warning_raised().to_egui();
    let glyph = theme.icon_glyph_size_sm.value();
    let caption = theme.font_size_caption.value();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
        ui.vertical(|ui| {
            ui.add_space(STRUCT_GAP_1.value());
            let (rect, _) = ui.allocate_exact_size(egui::vec2(glyph, glyph), egui::Sense::hover());
            tasty_icons::ALERT_TRIANGLE
                .image(glyph, warning)
                .paint_at(ui, rect);
        });
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(caption)
                    .line_height(Some(caption * theme.line_height_ui))
                    .color(warning),
            )
            .wrap(),
        );
    });
}

/// 판정 한 줄. 문구가 없어도 caption 한 줄 높이를 잡아 아래 버튼 줄이 움직이지 않는다.
fn check_line(ui: &mut egui::Ui, theme: &Theme, view: &ShellSetupView<'_>) {
    let caption = theme.font_size_caption.value();
    let glyph = theme.icon_glyph_size_sm.value();
    let height = (caption * theme.line_height_ui).max(glyph);
    let line = match view.check {
        ShellSetupCheck::Empty => None,
        ShellSetupCheck::Missing => Some((
            tasty_icons::ALERT_CIRCLE,
            theme.accent_danger_raised(),
            view.missing,
        )),
        ShellSetupCheck::NotShell => Some((
            tasty_icons::ALERT_CIRCLE,
            theme.accent_danger_raised(),
            view.not_shell,
        )),
        ShellSetupCheck::Directory => Some((
            tasty_icons::ALERT_CIRCLE,
            theme.accent_danger_raised(),
            view.directory,
        )),
        ShellSetupCheck::NotExecutable => Some((
            tasty_icons::ALERT_CIRCLE,
            theme.accent_danger_raised(),
            view.not_executable,
        )),
        ShellSetupCheck::Valid => Some((
            tasty_icons::CHECK,
            theme.accent_success_raised(),
            view.valid,
        )),
    };
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_height(height);
            let Some((icon, color, text)) = line else {
                return;
            };
            let color = color.to_egui();
            ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(glyph, glyph), egui::Sense::hover());
            icon.image(glyph, color).paint_at(ui, rect);
            ui.label(egui::RichText::new(text).size(caption).color(color));
        },
    );
}
