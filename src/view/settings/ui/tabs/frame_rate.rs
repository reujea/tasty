//! Terminal › Frame rate. 출력·입력이 유발하는 다시 그리기의 상한 두 행과 그 아래 캡션이다.
//!
//! 섹션 본문은 tasty-ui-widgets 의 공용 함수가 그리고, 이 파일은 번역 문구·모니터 조회·draft 읽기와
//! 쓰기만 맡는다.
//!
//! 저장은 설정 창의 다른 행처럼 Save 때 적용 경로(`cascade_settings_updated`)를 타서 열린 창 전부에
//! 바로 적용된다. 그래서 Performance 와 달리 적용 안내를 두지 않는다.
//!
//! 선택지는 연결된 모니터 중 가장 빠른 주사율까지다. 모니터 목록은 이 섹션을 그리는 프레임마다
//! 다시 읽는다. 설정 창은 입력이 있을 때만 그리고 트리거를 누른 프레임에 목록도 함께 그리므로,
//! 목록을 열 때마다 그때 연결된 모니터로 선택지를 만든다. 다른 섹션을 그리는 프레임은 읽지 않는다.

use std::sync::{Arc, Weak};

use crate::i18n::t;
use crate::settings::Settings;
use tasty_i18n::fill_args;
use tasty_settings::{RedrawFpsCopy, max_refresh_millihertz, redraw_fps_rows};
use tasty_ui_widgets::{FpsField, FrameRateCopy, frame_rate_section, vspace};

/// 설정 창 egui 컨텍스트에 두는 모니터 조회. 창이 이미 사라졌으면 보고 없음으로 본다.
#[derive(Clone)]
struct MonitorProbe(Arc<dyn Fn() -> Option<u32> + Send + Sync>);

fn probe_id() -> egui::Id {
    egui::Id::new("tasty_settings_frame_rate_monitor_probe")
}

/// 설정 창을 만들 때 한 번 부른다. 섹션이 그 창의 연결된 모니터를 그릴 때마다 읽게 한다.
pub fn install_monitor_probe(ctx: &egui::Context, window: &Arc<winit::window::Window>) {
    let window: Weak<winit::window::Window> = Arc::downgrade(window);
    let probe = MonitorProbe(Arc::new(move || {
        let window = window.upgrade()?;
        max_refresh_millihertz(
            window
                .available_monitors()
                .map(|m| m.refresh_rate_millihertz()),
        )
    }));
    ctx.data_mut(|d| d.insert_temp(probe_id(), probe));
}

/// 가장 빠른 연결 모니터의 주사율(millihertz). 조회가 없거나 보고가 없으면 `None`(60Hz 로 본다).
fn fastest_monitor(ctx: &egui::Context) -> Option<u32> {
    let probe: Option<MonitorProbe> = ctx.data(|d| d.get_temp(probe_id()));
    probe.and_then(|p| (p.0)())
}

/// 섹션 본문은 갤러리 견본과 같은 [`frame_rate_section`]이다. 여기서는 번역 문구와 연결된 모니터로
/// 선택지를 만들고 draft 의 두 값을 넘긴다.
pub fn draw_frame_rate_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    let monitor = fastest_monitor(ui.ctx());
    let list = RedrawFpsCopy {
        fps: t("settings.frame_rate.fps"),
        above_monitor: t("settings.frame_rate.fps_above_monitor"),
        match_monitor: t("settings.frame_rate.match_monitor"),
    };
    let copy = FrameRateCopy {
        output: t("settings.frame_rate.output"),
        output_hint: t("settings.frame_rate.output_desc"),
        input: t("settings.frame_rate.input"),
        input_hint: t("settings.frame_rate.input_desc"),
        caption: t("settings.frame_rate.caption"),
    };
    let perf = &mut settings.performance;
    let output = redraw_fps_rows(monitor, perf.output_redraw_fps, &list, fill_args);
    let input = redraw_fps_rows(monitor, perf.input_redraw_fps, &list, fill_args);
    vspace(ui, th.spacing_sm);
    frame_rate_section(
        ui,
        &th,
        &copy,
        FpsField {
            salt: "frame_rate_output",
            choices: &output,
            value: &mut perf.output_redraw_fps,
        },
        FpsField {
            salt: "frame_rate_input",
            choices: &input,
            value: &mut perf.input_redraw_fps,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::t_fmt;
    use crate::view::settings::ui::select_probe as probe;

    fn draw(settings: &mut Settings) -> impl FnMut(&mut egui::Ui) + '_ {
        move |ui| draw_frame_rate_tab(ui, settings)
    }

    fn fps(v: i64) -> String {
        t_fmt("settings.frame_rate.fps", &v.to_string())
    }

    fn above(v: i64) -> String {
        t_fmt("settings.frame_rate.fps_above_monitor", &v.to_string())
    }

    fn with_monitor(ctx: &egui::Context, mhz: Option<u32>) {
        let probe = MonitorProbe(Arc::new(move || mhz));
        ctx.data_mut(|d| d.insert_temp(probe_id(), probe));
    }

    /// 기본값은 두 트리거가 30·60 fps 이고, 다른 값을 고르면 그 i64 로 바뀐다.
    #[test]
    fn picking_a_listed_value_writes_its_number() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let out = probe::frame(&ctx, &mut draw(&mut settings));
        assert!(probe::trigger_around(&out, &fps(30)).is_some());
        assert!(probe::trigger_around(&out, &fps(60)).is_some());
        probe::pick(&ctx, &mut draw(&mut settings), &fps(30), &fps(15));
        assert_eq!(settings.performance.output_redraw_fps, 15);
        let match_monitor = t("settings.frame_rate.match_monitor");
        probe::pick(&ctx, &mut draw(&mut settings), &fps(60), match_monitor);
        assert_eq!(settings.performance.input_redraw_fps, -1);
    }

    /// 60Hz 에서 144 저장 값은 "above monitor" 트리거이고, 그 행을 다시 골라도 값은 그대로다.
    #[test]
    fn a_saved_value_above_the_monitor_survives_picking_its_row() {
        let ctx = egui::Context::default();
        with_monitor(&ctx, Some(60_000));
        let mut settings = Settings::default();
        settings.performance.output_redraw_fps = 144;
        let out = probe::frame(&ctx, &mut draw(&mut settings));
        assert!(probe::trigger_around(&out, &above(144)).is_some());
        probe::pick(&ctx, &mut draw(&mut settings), &above(144), &above(144));
        assert_eq!(settings.performance.output_redraw_fps, 144);
    }

    /// 모니터가 144Hz 를 보고하면 144 는 목록 안 값이다.
    #[test]
    fn a_faster_monitor_lists_the_saved_value_plainly() {
        let ctx = egui::Context::default();
        with_monitor(&ctx, Some(144_000));
        let mut settings = Settings::default();
        settings.performance.output_redraw_fps = 144;
        let out = probe::frame(&ctx, &mut draw(&mut settings));
        assert!(probe::trigger_around(&out, &fps(144)).is_some());
        assert!(probe::trigger_around(&out, &above(144)).is_none());
    }
}
