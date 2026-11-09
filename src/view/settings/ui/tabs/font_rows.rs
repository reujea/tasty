//! 글꼴 행의 오류 표시. 판정은 미리보기 등록(UI 행)이나 포커스가 빠진 뒤의 터미널 판정이 남긴 결과를
//! 읽기만 하므로 행을 그리는 동안 파일을 읽지 않는다.

use std::collections::HashMap;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{FieldErrorLine, Input, field_error_state};

use crate::adapters::ui::font_judge::{
    FontLookup, FontOrigin, FontRenderer, FontVerdict, judge_font,
};
use crate::adapters::ui::font_registry::RejectedFont;
use crate::i18n::t;
use crate::settings::{AppearanceSettings, EffectiveFont};

/// 미리보기에 등록된 글꼴과 행이 읽을 판정 결과. 설정 창이 열려 있는 동안 유지한다.
#[derive(Default)]
pub struct PreviewFonts {
    /// 미리보기 슬롯마다 등록한 값의 키.
    loaded: HashMap<String, String>,
    /// 마지막 등록에서 거절된 그 슬롯의 글꼴들.
    rejected: Vec<RejectedFont>,
    /// 터미널 판정을 마친 값의 키와 결과.
    terminal: Option<(String, FontVerdict)>,
    lookup: FontLookup,
}

fn font_key(family: &str, custom: &str) -> String {
    format!("{family}|{custom}")
}

pub(super) fn preview_slot(salt: &str) -> String {
    format!("preview_{salt}")
}

impl PreviewFonts {
    #[cfg(test)]
    pub(super) fn terminal_key(&self) -> Option<&str> {
        self.terminal.as_ref().map(|(key, _)| key.as_str())
    }

    /// 이 슬롯에 이 값이 이미 등록돼 있는지.
    pub(super) fn is_loaded(&self, slot: &str, eff: &EffectiveFont) -> bool {
        self.loaded.get(slot).map(String::as_str)
            == Some(font_key(&eff.font_family, &eff.custom_font_path).as_str())
    }

    /// 전체 구성을 새로 설치했으므로 다른 슬롯의 등록은 사라졌다. 탭에 돌아올 때 다시 등록된다.
    pub(super) fn record_build(
        &mut self,
        slot: &str,
        eff: &EffectiveFont,
        rejected: Vec<RejectedFont>,
    ) {
        self.loaded.clear();
        self.loaded.insert(
            slot.to_owned(),
            font_key(&eff.font_family, &eff.custom_font_path),
        );
        self.rejected = rejected.into_iter().filter(|r| r.slot == slot).collect();
        for font in &self.rejected {
            tracing::debug!(
                "preview refused {:?} font {}: {}",
                font.origin,
                font.value,
                font.reason
            );
        }
    }

    /// 미리보기를 그릴 family. 슬롯에 값이 등록돼 있으면 그 family 로 그린다. 거절된 글꼴은 등록되지 않아
    /// family 의 다음 글꼴이 그린다. 값이 바뀌었으면 전체 구성을 다시 설치하고 family 는 다음 프레임부터 쓸 수
    /// 있으므로 이번 프레임은 고정폭으로 그린다.
    pub(super) fn family_for(
        &mut self,
        ctx: &egui::Context,
        appearance: &AppearanceSettings,
        slot_name: &str,
        eff: &EffectiveFont,
    ) -> egui::FontFamily {
        if eff.font_family.is_empty() && eff.custom_font_path.is_empty() {
            return egui::FontFamily::Monospace;
        }
        if self.is_loaded(slot_name, eff) {
            return egui::FontFamily::Name(slot_name.into());
        }
        let build = crate::adapters::ui::font_registry::build_font_definitions(
            appearance,
            Some((slot_name, eff)),
        );
        ctx.set_fonts(build.fonts);
        self.record_build(slot_name, eff, build.rejected);
        // 행은 이미 그렸으므로 판정을 보이려면 다음 프레임이 필요하다.
        ctx.request_repaint();
        egui::FontFamily::Monospace
    }

    /// 터미널 행에 override 가 있을 때만 판정한다. 줄을 그릴 행이 없으면 파일을 읽지 않는다.
    pub(super) fn settle_terminal_row(
        &mut self,
        ctx: &egui::Context,
        appearance: &AppearanceSettings,
        eff: &EffectiveFont,
        active: bool,
    ) {
        let ov = &appearance.terminal_font;
        if ov.font_family.is_some() || ov.custom_font_path.is_some() {
            self.settle_terminal(ctx, eff, active);
        }
    }

    /// 입력을 마친 값을 터미널 경로로 판정한다. 입력 중이거나 이미 판정한 값이면 아무것도 하지 않는다.
    /// 행은 이미 그려졌으므로 다음 프레임을 요청한다.
    pub(super) fn settle_terminal(
        &mut self,
        ctx: &egui::Context,
        eff: &EffectiveFont,
        active: bool,
    ) {
        let key = font_key(&eff.font_family, &eff.custom_font_path);
        if active || self.terminal.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let verdict = judge_font(
            FontRenderer::Terminal,
            &eff.font_family,
            &eff.custom_font_path,
            &mut self.lookup,
        );
        self.terminal = Some((key, verdict));
        ctx.request_repaint();
    }
}

/// 한 행의 판정. `pending` 은 지금 값의 판정이 아직 없다는 뜻이고, 그동안에는 입력 중처럼 다룬다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RowVerdict {
    pub file: bool,
    pub family: bool,
    pub pending: bool,
}

/// 행이 판정을 읽는 곳.
#[derive(Clone, Copy)]
pub(super) enum RowJudge<'a> {
    Ui {
        fonts: &'a PreviewFonts,
        slot: &'a str,
    },
    Terminal {
        fonts: &'a PreviewFonts,
    },
}

impl<'a> RowJudge<'a> {
    /// 터미널 행은 터미널 판정을, 그 밖의 행은 슬롯의 UI 판정을 읽는다.
    pub(super) fn for_surface(terminal: bool, fonts: &'a PreviewFonts, slot: &'a str) -> Self {
        if terminal {
            Self::Terminal { fonts }
        } else {
            Self::Ui { fonts, slot }
        }
    }

    pub(super) fn verdict(self, family: &str, custom: &str) -> RowVerdict {
        if family.is_empty() && custom.is_empty() {
            return RowVerdict::default();
        }
        let key = font_key(family, custom);
        match self {
            Self::Ui { fonts, slot } => {
                if fonts.loaded.get(slot) != Some(&key) {
                    return RowVerdict {
                        pending: true,
                        ..Default::default()
                    };
                }
                let refused = |origin, value: &str| {
                    fonts
                        .rejected
                        .iter()
                        .any(|r| r.slot == slot && r.origin == origin && r.value == value)
                };
                RowVerdict {
                    file: refused(FontOrigin::Custom, custom),
                    family: refused(FontOrigin::Named, family),
                    pending: false,
                }
            }
            Self::Terminal { fonts } => match &fonts.terminal {
                Some((judged, verdict)) if *judged == key => RowVerdict {
                    file: verdict.custom.is_some(),
                    family: verdict.family.is_some(),
                    pending: false,
                },
                _ => RowVerdict {
                    pending: true,
                    ..Default::default()
                },
            },
        }
    }
}

/// 사용자가 값을 고치는 custom 파일 입력. 입력 응답과 오류 줄을 돌려준다.
pub(super) fn custom_font_field(
    ui: &mut egui::Ui,
    th: &Theme,
    value: &mut String,
    family: &str,
    judge: RowJudge<'_>,
    salt: &str,
) -> (egui::Response, Option<FieldErrorLine<'static>>) {
    let id = egui::Id::new(("font_custom_field", salt));
    let before = judge.verdict(family, value);
    // 좁은 설정 창에서 남은 폭을 넘으면 체크박스를 덮는다.
    let width = th.field_width_lg.min(LogicalPx(ui.available_width()));
    let response = Input::new()
        .id(id)
        .mono(true)
        .width(width.value())
        .invalid_at_rest(before.file && !before.pending)
        .full_value_tooltip(true)
        .show(ui, th, value);
    let after = judge.verdict(family, value);
    let state = field_error_state(
        ui.ctx(),
        id,
        response.has_focus() || after.pending,
        after.file,
    );
    let line = state.line(t("settings.appearance.font.custom_font_rejected"));
    (response, line)
}

/// family 콤보 아래 오류 줄. 콤보에는 빨간 테두리가 없다.
pub(super) fn family_error_line(
    ctx: &egui::Context,
    salt: &str,
    open: bool,
    verdict: RowVerdict,
) -> Option<FieldErrorLine<'static>> {
    let id = egui::Id::new(("font_family_field", salt));
    field_error_state(ctx, id, open || verdict.pending, verdict.family)
        .line(t("settings.appearance.font.family_rejected"))
}
