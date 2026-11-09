//! 글꼴 미리보기의 글꼴 선택.

use std::collections::HashMap;

use crate::settings::{AppearanceSettings, EffectiveFont};

/// 미리보기를 그릴 family 를 정하고 필요하면 글꼴 구성을 다시 설치한다.
pub(super) fn preview_font_family(
    ctx: &egui::Context,
    appearance: &AppearanceSettings,
    slot_name: &str,
    eff: &EffectiveFont,
    preview_font_loaded: &mut HashMap<String, String>,
) -> egui::FontFamily {
    // Decide which egui FontFamily to render text in. We try to load the
    // requested family into a per-slot named family the first time we see it,
    // and remember success/failure in `preview_font_loaded` so we don't retry.
    if eff.font_family.is_empty() && eff.custom_font_path.is_empty() {
        egui::FontFamily::Monospace
    } else {
        let key = format!("{}|{}", eff.font_family, eff.custom_font_path);
        let failed_marker = format!("\x00:{}", key);
        let cached = preview_font_loaded
            .get(slot_name)
            .cloned()
            .unwrap_or_default();
        if cached == key {
            egui::FontFamily::Name(slot_name.into())
        } else if cached == failed_marker {
            egui::FontFamily::Monospace
        } else {
            // First attempt this frame: rebuild the full FontDefinitions
            // (surface families + this preview slot) and install it.
            let fonts = crate::adapters::ui::font_registry::build_font_definitions(
                appearance,
                Some((slot_name, eff)),
            );
            ctx.set_fonts(fonts);
            // set_fonts() replaces the entire FontDefinitions, so other
            // preview slots are no longer registered. Clear them so they
            // get re-loaded when their tab is revisited.
            preview_font_loaded.retain(|k, _| k == slot_name);
            preview_font_loaded.insert(slot_name.to_owned(), key);
            // The family won't be available until the next frame; fall back
            // to Monospace this frame.
            egui::FontFamily::Monospace
        }
    }
}
