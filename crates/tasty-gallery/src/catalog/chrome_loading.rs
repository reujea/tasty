//! 부팅·종료 화면의 로고, 스피너, 진행 문구 예제.
//! 본체와 공통 브랜드 위젯·Theme 값을 사용하며 두 화면은 문구만 다르게 그린다.

mod boot_error;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::brand::{self};
use tasty_ui_widgets::{ShellSetupCheck, ShellSetupView, Spinner, shell_setup_screen};

use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage};

// 크기가 다른 예제 창에서도 로딩 요소의 크기를 유지하는지 비교한다.

/// 기본 크기 예제 창.
const CANVAS_DEFAULT: (f32, f32) = (1280.0, 720.0);
/// 기본 크기 예제를 보여 주는 배율. 시안 `BootFrame z={0.6}`처럼 1280×720 창을 60%로 줄여
/// 문서 칸에 넣는다. 크기 토큰도 같은 배율의 Theme 에서 읽는다.
const DEFAULT_PREVIEW_ZOOM: f32 = 0.6;
/// 최소 크기 예제 창. 문구 없음·Latte 예제에서도 사용한다.
const CANVAS_MIN: (f32, f32) = (640.0, 480.0);
/// 진행 문구들을 나란히 비교할 예제 창.
const CANVAS_MULTI: (f32, f32) = (320.0, 240.0);

/// 요소의 크기는 유지하고 주어진 창 안에서 수직 중앙에 배치한다.
fn draw_frame(ui: &mut egui::Ui, theme: &Theme, canvas: egui::Vec2, phase_text: Option<&str>) {
    let (rect, _) = ui.allocate_exact_size(canvas, egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.bg_app().to_egui());

    let content_height = theme.loading_screen_wordmark_icon_size().value()
        + theme.spacing_xl.value()
        + theme.loading_screen_spinner_size().value()
        + theme.spacing_lg.value()
        + theme.loading_screen_phase_slot_height().value();
    let top_pad = ((canvas.y - content_height) / 2.0).max(0.0);

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    child.add_space(top_pad);
    brand::draw_wordmark(
        &mut child,
        theme,
        theme.loading_screen_wordmark_icon_size(),
        theme.loading_screen_wordmark_font_size(),
        theme.loading_lockup_tracking(),
    );
    child.add_space(theme.spacing_xl.value());
    Spinner::new()
        .size(theme.loading_screen_spinner_size().value())
        .color(theme.accent_primary_raised().to_egui())
        .show(&mut child, theme);
    child.add_space(theme.spacing_lg.value());
    let (slot_rect, _) = child.allocate_exact_size(
        egui::vec2(canvas.x, theme.loading_screen_phase_slot_height().value()),
        egui::Sense::hover(),
    );
    if let Some(text) = phase_text {
        child.painter().text(
            slot_rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(theme.font_size_body.value()),
            theme.text_muted().to_egui(),
        );
    }
}

/// 기본 크기 창을 시안 배율로 줄여 그린다.
fn draw_default_preview(ui: &mut egui::Ui, theme: &Theme, phase_text: &str) {
    let zoomed = Theme::with_colors_and_zoom(
        theme.to_colors(),
        theme.is_light,
        theme.ui_zoom * DEFAULT_PREVIEW_ZOOM,
    );
    let canvas = egui::vec2(CANVAS_DEFAULT.0, CANVAS_DEFAULT.1) * DEFAULT_PREVIEW_ZOOM;
    draw_frame(ui, &zoomed, canvas, Some(phase_text));
}

pub fn draw_default(ui: &mut egui::Ui, theme: &Theme) {
    draw_default_preview(ui, theme, crate::i18n::t("boot.phase_gpu_init"));
    meta(
        ui,
        theme,
        &[
            ("window", "1280×720 default · shown at 60%"),
            ("phase", "GpuInit"),
        ],
        &[
            TokenChip::new("bg-app", "full surface", theme.bg_app().to_egui()),
            TokenChip::new(
                "accent-primary",
                "spinner arc",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "brand-melon-flesh",
                "wordmark dot",
                theme.brand_melon_flesh().to_egui(),
            ),
            TokenChip::without_color("spinner-duration", "900ms rotation"),
            TokenChip::without_color("space-xl / -lg", "stack gaps"),
        ],
    );
}

pub fn draw_min(ui: &mut egui::Ui, theme: &Theme) {
    draw_frame(
        ui,
        theme,
        egui::vec2(CANVAS_MIN.0, CANVAS_MIN.1),
        Some("Loading plugins…"),
    );
    meta(
        ui,
        theme,
        &[("window", "640×480 minimum"), ("phase", "WaitingPlugins")],
        &[],
    );
    note(
        ui,
        theme,
        "Layout is size-invariant — same centered stack at both window extremes, no responsive scaling.",
    );
}

pub fn draw_phases(ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        for text in [
            crate::i18n::t("boot.phase_gpu_init"),
            crate::i18n::t("boot.phase_waiting_plugins"),
            crate::i18n::t("boot.phase_restoring_layout"),
        ] {
            draw_frame(
                ui,
                theme,
                egui::vec2(CANVAS_MULTI.0, CANVAS_MULTI.1),
                Some(text),
            );
        }
    });
    meta(
        ui,
        theme,
        &[
            ("GpuInit / WaitingEngine", "\"Initializing graphics…\""),
            ("WaitingPlugins", "\"Loading plugins…\""),
            ("RestoringLayout", "\"Restoring layout…\""),
        ],
        &[
            TokenChip::without_color("size-16", "fixed phase-slot height"),
            TokenChip::new("text-muted", "phase color", theme.text_muted().to_egui()),
        ],
    );
}

/// 문구가 없어도 해당 영역의 높이를 유지한다.
pub fn draw_no_text(ui: &mut egui::Ui, theme: &Theme) {
    draw_frame(ui, theme, egui::vec2(CANVAS_MIN.0, CANVAS_MIN.1), None);
    note(
        ui,
        theme,
        "First install can skip RestoringLayout — the phase slot stays reserved but empty, no layout shift.",
    );
}

/// 부팅 화면은 새 스피너를 만들지 않고 공용 Spinner를 한 단계 큰 크기로 쓴다.
pub fn draw_spinner_hero(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        let size = theme.loading_screen_spinner_size().value();
        let color = theme.accent_primary().to_egui();
        Spinner::new().size(size).color(color).show(ui, theme);
        Spinner::new()
            .size(size)
            .color(color)
            .reduced_motion(true)
            .show(ui, theme);
        ui.label(
            egui::RichText::new("animated · reduced-motion (3-dot fallback)")
                .monospace()
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    });
    meta(
        ui,
        theme,
        &[
            ("size", "32px (catalog 16 → hero) · viewBox 24"),
            ("stroke", "3px · arc round cap"),
            (
                "arc / track",
                "90° accent-primary / full-circle currentColor @0.22",
            ),
            (
                "rotation",
                "spinner-duration 900ms linear ∞ (constant velocity)",
            ),
            ("reduced-motion", "arc frozen / Spinner 3-dot fallback"),
        ],
        &[
            TokenChip::new(
                "spinner-indicator",
                "= accent-primary",
                theme.spinner_indicator().to_egui(),
            ),
            TokenChip::without_color("spinner-track", "0.22 track"),
            TokenChip::without_color("spinner-duration", "900ms"),
        ],
    );
}

/// 툴바의 테마 선택과 무관하게 Latte로 그린다.
pub fn draw_latte(ui: &mut egui::Ui, _theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    draw_frame(
        ui,
        &latte,
        egui::vec2(CANVAS_MIN.0, CANVAS_MIN.1),
        Some("Restoring layout…"),
    );
    note(
        ui,
        &latte,
        "GPU clear color reads the resolved theme's bg-app — Latte follows the saved theme, not a hardcoded dark.",
    );
}

/// 종료 단계 비교 칸을 줄여 보이는 배율. 시안 `BootFrame w={640} h={480} z={0.5}`.
const SHUTDOWN_PHASE_ZOOM: f32 = 0.5;
/// 종료 단계 문구. 본체 `ShutdownPhase::text_key`의 키를 같은 순서로 읽는다.
fn shutdown_phases() -> [&'static str; 4] {
    [
        crate::i18n::t("shutdown.phase_saving_layout"),
        crate::i18n::t("shutdown.phase_stopping_background_worker"),
        crate::i18n::t("shutdown.phase_closing_surfaces"),
        crate::i18n::t("shutdown.phase_stopping_plugins"),
    ]
}

/// `canvas` 크기 창을 `zoom` 배율로 줄여 그린다. 크기 토큰도 같은 배율의 Theme 에서 읽는다.
fn draw_scaled_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    canvas: (f32, f32),
    zoom: f32,
    phase_text: &str,
) {
    let zoomed =
        Theme::with_colors_and_zoom(theme.to_colors(), theme.is_light, theme.ui_zoom * zoom);
    draw_frame(
        ui,
        &zoomed,
        egui::vec2(canvas.0, canvas.1) * zoom,
        Some(phase_text),
    );
}

/// 시안 "Shutdown screen": 부팅과 같은 화면에 종료 단계 문구만 다르다.
pub fn draw_shutdown(ui: &mut egui::Ui, theme: &Theme) {
    draw_default_preview(ui, theme, shutdown_phases()[0]);
    ui.add_space(theme.spacing_lg.value());
    egui::Grid::new("shutdown_phase_grid")
        .num_columns(2)
        .spacing(egui::vec2(
            theme.spacing_lg.value(),
            theme.spacing_lg.value(),
        ))
        .show(ui, |ui| {
            for (i, text) in shutdown_phases().iter().enumerate() {
                draw_scaled_frame(ui, theme, CANVAS_MIN, SHUTDOWN_PHASE_ZOOM, text);
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    meta(
        ui,
        theme,
        &[
            (
                "surface · stack",
                "identical to boot (lockup → space-xl → spinner → space-lg → phase slot)",
            ),
            ("1 · SavingLayout", "Saving layout…"),
            (
                "2 · ReclaimingBootWorker",
                crate::i18n::t("shutdown.phase_stopping_background_worker"),
            ),
            ("3 · ClosingSurfaces", "Closing surfaces…"),
            ("4 · StoppingPlugins", "Stopping plugins…"),
            ("nothing to wait for", "no frame — the window closes"),
            ("theme", "follows the saved theme, like boot"),
        ],
        &[
            TokenChip::new("bg-app", "full surface", theme.bg_app().to_egui()),
            TokenChip::new(
                "accent-primary",
                "spinner arc",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new("text-muted", "phase", theme.text_muted().to_egui()),
        ],
    );
    note(
        ui,
        theme,
        "No new tokens. Phase copy is i18n (shutdown.phase_saving_layout · \
         shutdown.phase_stopping_background_worker · shutdown.phase_closing_surfaces · \
         shutdown.phase_stopping_plugins); keep the trailing ellipsis character.",
    );
}

/// 셸 설정 예제 한 장의 OS. Windows만 Git Bash 안내 줄과 Windows 경로를 쓴다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SetupOs {
    Win,
    Mac,
}

/// 디자인 `ShellSetupFrame`의 판정별 입력값. Windows는 Git Bash 조건(`NotShell`), macOS·Linux는 실행 가능 여부
/// (`Directory`·`NotExecutable`)로 막는다. macOS는 fish처럼 bash·zsh가 아닌 셸도 쓸 수 있다.
fn setup_value(os: SetupOs, check: ShellSetupCheck) -> &'static str {
    match (os, check) {
        (_, ShellSetupCheck::Empty) => "",
        (SetupOs::Win, ShellSetupCheck::Missing) => "C:/Program Files/Git/bin/bash.exe",
        (SetupOs::Mac, ShellSetupCheck::Missing) => "/usr/local/bin/zsh",
        (SetupOs::Win, ShellSetupCheck::NotShell) => "C:/Windows/System32/cmd.exe",
        (SetupOs::Mac, ShellSetupCheck::Directory) => "/opt/homebrew/bin",
        (SetupOs::Mac, ShellSetupCheck::NotExecutable) => "/Users/me/bin/fish",
        (SetupOs::Win, ShellSetupCheck::Valid) => "D:/Tools/Git/bin/bash.exe",
        (SetupOs::Mac, ShellSetupCheck::Valid) => "/opt/homebrew/bin/fish",
        // 그 OS에서 나오지 않는 판정이다. 예제 목록에 넣지 않는다.
        (SetupOs::Mac, ShellSetupCheck::NotShell)
        | (SetupOs::Win, ShellSetupCheck::Directory | ShellSetupCheck::NotExecutable) => "",
    }
}

/// 셸 설정 예제 창 한 장. 본체와 같은 `shell_setup_screen`을 최소 크기 창에 그린다.
fn draw_shell_setup_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    index: usize,
    os: SetupOs,
    check: ShellSetupCheck,
) {
    let bw = theme.border_width.value();
    let size = egui::vec2(CANVAS_MIN.0 + bw * 2.0, CANVAS_MIN.1 + bw * 2.0);
    let (outer, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let radius = theme.corner_radius_lg.value();
    ui.painter()
        .rect_filled(outer, radius, theme.border_strong().to_egui());
    let inner = outer.shrink(bw);
    let mut path = setup_value(os, check).to_owned();
    let win = os == SetupOs::Win;
    let view = ShellSetupView {
        title: "Choose a shell",
        subtitle: crate::i18n::t("boot.shell_setup.subtitle"),
        git_bash_notice: win.then_some(crate::i18n::t("boot.shell_setup.git_bash_missing")),
        placeholder: if win {
            "C:/Program Files/Git/bin/bash.exe"
        } else {
            "/bin/zsh"
        },
        missing: crate::i18n::t("boot.shell_setup.check_missing"),
        not_shell: crate::i18n::t("boot.shell_setup.check_not_shell"),
        directory: crate::i18n::t("boot.shell_setup.check_directory"),
        not_executable: crate::i18n::t("boot.shell_setup.check_not_executable"),
        valid: if win {
            crate::i18n::t("boot.shell_setup.check_valid_git_bash")
        } else {
            crate::i18n::t("boot.shell_setup.check_valid")
        },
        quit: "Quit",
        confirm: "Use this shell",
        check,
    };
    let mut frame = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .id_salt(("shell_setup_frame", index)),
    );
    frame.set_clip_rect(inner.intersect(ui.clip_rect()));
    // 예제는 판정을 고정해 보이므로 입력과 눌림을 버린다.
    let _output = shell_setup_screen(&mut frame, theme, &view, &mut path);
}

/// Windows 4판정, macOS 3판정(디렉터리 · 실행 권한 없음 · fish 확정), Latte 2장.
pub fn draw_shell_setup(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let frames = [
        (theme, SetupOs::Win, ShellSetupCheck::Empty),
        (theme, SetupOs::Win, ShellSetupCheck::Missing),
        (theme, SetupOs::Win, ShellSetupCheck::NotShell),
        (theme, SetupOs::Win, ShellSetupCheck::Valid),
        (theme, SetupOs::Mac, ShellSetupCheck::Directory),
        (theme, SetupOs::Mac, ShellSetupCheck::NotExecutable),
        (theme, SetupOs::Mac, ShellSetupCheck::Valid),
        (&latte, SetupOs::Win, ShellSetupCheck::Missing),
        (&latte, SetupOs::Mac, ShellSetupCheck::Valid),
    ];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing =
            egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
        for (index, (th, os, check)) in frames.into_iter().enumerate() {
            draw_shell_setup_frame(ui, th, index, os, check);
        }
    });
    meta(
        ui,
        theme,
        &[
            ("form width", "360 · size-360"),
            (
                "stack",
                "lockup → (space-xl) → title · sub · input · validation · buttons (space-sm)",
            ),
            ("title", "14 / 600 · text-primary"),
            (
                "validation (2026-10-06)",
                "empty → blank line, height reserved · No file at this path (every OS) · Windows: Not Git Bash. Use bash.exe from Git for Windows · macOS/Linux: folder · no permission to run (danger, any shell name such as fish is fine) · Shell found (success, macOS and Linux) · Git Bash found (success, Windows)",
            ),
            (
                "Git Bash notice",
                "Windows only · caption line alertTriangle + warning ink · between sub and Input · no box",
            ),
            (
                "confirm",
                "Button primary md · Use this shell · disabled unless valid · Enter = confirm when valid",
            ),
            (
                "cancel → Quit",
                "Button secondary md · labelled Quit because it exits the app",
            ),
            (
                "no card",
                "the host's 440 card, 12px literal and 32 input go: control-height Input, caption type, size-360 form",
            ),
        ],
        &[
            TokenChip::new(
                "accent-success",
                "valid line",
                theme.accent_success().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "invalid line",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new(
                "state-disabled-fg",
                "disabled confirm ink",
                egui::Color32::from(theme.state_disabled_fg()),
            ),
        ],
    );
    // 같은 구역의 두 번째 Spec. 구역 목록(catalog.rs)은 Spec 하나로 둔다.
    boot_error::draw(ui, theme);
}
