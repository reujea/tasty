//! MainViewState·CoreState·GpuState에서 디버그 조회용 JSON을 만든다.

use serde_json::{Value, json};

use crate::gpu::GpuState;
use crate::state::MainViewState;

pub fn collect(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    gpu: Option<&GpuState>,
    ime_active: bool,
) -> Value {
    let mut info = serde_json::Map::new();

    info.insert("workspace_count".into(), json!(engine.workspaces().len()));
    info.insert(
        "active_workspace".into(),
        json!(state.active_workspace_index(engine)),
    );

    if let Some(gpu) = gpu {
        info.insert("scale_factor".into(), json!(gpu.scale_factor()));
        info.insert("cell_width".into(), json!(gpu.cell_width()));
        info.insert("cell_height".into(), json!(gpu.cell_height()));
        info.insert(
            "terminal_font_updates".into(),
            json!(gpu.terminal_font_updates()),
        );
        let size = gpu.size();
        info.insert("viewport_width".into(), json!(size.width));
        info.insert("viewport_height".into(), json!(size.height));
    }

    let appearance = &engine.settings.appearance;
    let term_eff = appearance.effective_terminal_font();
    let md_eff = appearance.effective_font_for_kind("markdown");
    info.insert(
        "default_font_size".into(),
        json!(appearance.default_font.font_size),
    );
    info.insert(
        "default_font_family".into(),
        json!(&appearance.default_font.font_family),
    );
    info.insert("terminal_font_size".into(), json!(term_eff.font_size));
    info.insert("terminal_font_family".into(), json!(term_eff.font_family));
    info.insert("markdown_font_size".into(), json!(md_eff.font_size));
    info.insert("markdown_font_family".into(), json!(md_eff.font_family));

    info.insert("ime_active".into(), json!(ime_active));
    if let Some(gpu) = gpu {
        info.insert("egui_ime_allowed".into(), json!(gpu.egui_ime_allowed()));
    }

    // egui의 실제 배율은 GPU scale_factor와 다를 수 있다.
    if let Some(gpu) = gpu {
        info.insert(
            "egui_pixels_per_point".into(),
            json!(gpu.egui_pixels_per_point()),
        );
        info.insert("egui_zoom_factor".into(), json!(gpu.egui_zoom_factor()));
        let (cfg_w, cfg_h) = gpu.surface_config_size();
        info.insert("surface_config_width".into(), json!(cfg_w));
        info.insert("surface_config_height".into(), json!(cfg_h));
    }

    info.insert("tab_bar_height".into(), json!(state.tab_bar_height));
    info.insert("sidebar_width".into(), json!(state.sidebar_width));

    Value::Object(info)
}
