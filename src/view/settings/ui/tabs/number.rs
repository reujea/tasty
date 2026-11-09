//! 설정 창의 숫자 입력과 단위 표시.
//! 입력 중에는 보정하지 않고 포커스를 잃거나 Enter를 눌렀을 때 범위와 눈금을 적용한다.
//! 사용자가 고친 값이 범위 밖이면 경고와 최종 반영될 값을 표시한다. 저장 값을 그대로 둔 칸은
//! 소수·정수 칸 모두 범위 밖이어도 경고하거나 바꾸지 않는다. 정수 칸은 값 전체를 담도록 폭이 자란다.

use crate::i18n::t_args;
use tasty_type_appearance::theme::Theme;

/// 호출자가 지정하는 숫자 범위·자릿수·단위.
pub(super) struct NumberSpec<'a> {
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// 확정 시 스냅할 눈금. `None` 이면 스냅하지 않는다.
    pub step: Option<f64>,
    /// 표시 소수 자릿수. `0` 이면 정수 표기이며 칸 폭이 값에 맞춰 자란다.
    pub decimals: usize,
    /// 필드 **밖**에 muted 로 찍는 단위. 타이핑 대상이 아니다.
    pub suffix: Option<&'a str>,
    /// 단위를 `font-label` 로 표시할지 여부.
    pub suffix_label: bool,
    /// 칸 뒤 단위(`font-label`, `number-field-unit-fg`). `suffix` 와 함께 쓰지 않는다.
    pub unit: Option<&'a str>,
    /// 칸의 글자가 이 값이면 단위 자리에 단위 대신 이 설명을 적는다(예: -1 의 "no limit").
    pub no_limit: Option<(i128, &'a str)>,
    /// 범위 밖일 때 칸 아래에 적는 범위 문장(`number-field-note-fg`). 없으면 범위와 확정될 값을
    /// `accent-danger` 로 적는다. 문장의 세 자리표시는 min·max·확정값으로 채운다.
    pub range_note: Option<&'a str>,
    /// 값 정렬.
    pub align: egui::Align,
    pub enabled: bool,
}

impl<'a> NumberSpec<'a> {
    /// 정수 칸의 기본형 — 눈금·소수 없음.
    pub fn int(min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            step: None,
            decimals: 0,
            suffix: None,
            suffix_label: false,
            unit: None,
            no_limit: None,
            range_note: None,
            align: egui::Align::RIGHT,
            enabled: true,
        }
    }

    pub fn unit(mut self, unit: &'a str) -> Self {
        self.unit = Some(unit);
        self
    }

    pub fn no_limit(mut self, value: i128, label: &'a str) -> Self {
        self.no_limit = Some((value, label));
        self
    }

    pub fn range_note(mut self, note: &'a str) -> Self {
        self.range_note = Some(note);
        self
    }

    pub fn align(mut self, align: egui::Align) -> Self {
        self.align = align;
        self
    }

    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = Some(suffix);
        self
    }

    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub(super) fn format(&self, v: f64) -> String {
        format!("{v:.*}", self.decimals)
    }
}

/// 입력을 확정한 결과. 편집 중에는 사용하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Commit {
    /// 숫자로 안 읽힌다 — 마지막 확정값을 그대로 둔다.
    Unparsed,
    /// 이 값으로 확정한다. 범위·눈금을 이미 지났다.
    Value(f64),
}

/// 입력 문자열을 최종 숫자 값으로 변환한다.
pub(super) fn commit(text: &str, spec: &NumberSpec<'_>) -> Commit {
    let Ok(parsed) = text.trim().parse::<f64>() else {
        return Commit::Unparsed;
    };
    if !parsed.is_finite() {
        return Commit::Unparsed;
    }
    Commit::Value(settle(parsed, spec))
}

/// 눈금 스냅 → 범위 clamp. 순서가 이 방향인 이유는 눈금이 범위 밖으로 내보낼 수 있기
/// 때문이다(1.0~10.0 · 눈금 0.5 에서 10.3 은 10.5 로 스냅됐다가 10.0 으로 잘린다).
fn settle(v: f64, spec: &NumberSpec<'_>) -> f64 {
    let mut out = v;
    if let Some(step) = spec.step
        && step > 0.0
    {
        out = (out / step).round() * step;
    }
    if let Some(lo) = spec.min {
        out = out.max(lo);
    }
    if let Some(hi) = spec.max {
        out = out.min(hi);
    }
    out
}

/// 현재 입력을 확정하면 보정이 필요한지 확인한다.
fn out_of_range(text: &str, spec: &NumberSpec<'_>) -> Option<f64> {
    match commit(text, spec) {
        Commit::Value(settled) => {
            let typed = text.trim().parse::<f64>().ok()?;
            (settled != typed).then_some(settled)
        }
        Commit::Unparsed => None,
    }
}

/// 정수 확정. 정수 표기는 `f64` 를 거치지 않아 19자리 값도 그대로 남는다. 정수가 아닌 표기
/// (`1e3`, `12.7`)는 실수 확정을 거쳐 0 쪽으로 자른다.
fn commit_int(text: &str, spec: &NumberSpec<'_>) -> Option<i128> {
    match text.trim().parse::<i128>() {
        Ok(v) => Some(clamp_int(v, spec)),
        Err(_) => match commit(text, spec) {
            Commit::Value(v) => Some(v as i128),
            Commit::Unparsed => None,
        },
    }
}

/// 범위 끝은 정수 칸에서 정수다.
fn clamp_int(v: i128, spec: &NumberSpec<'_>) -> i128 {
    let mut out = v;
    if let Some(lo) = spec.min {
        out = out.max(lo.ceil() as i128);
    }
    if let Some(hi) = spec.max {
        out = out.min(hi.floor() as i128);
    }
    out
}

/// 정수 칸에서 확정하면 보정될 값.
fn out_of_range_int(text: &str, spec: &NumberSpec<'_>) -> Option<i128> {
    match text.trim().parse::<i128>() {
        Ok(v) => {
            let settled = clamp_int(v, spec);
            (settled != v).then_some(settled)
        }
        Err(_) => out_of_range(text, spec).map(|v| v as i128),
    }
}

/// 경고 한 줄 — 범위와 **확정될 값**을 함께 적는다.
fn range_line(spec: &NumberSpec<'_>, settled: &str) -> String {
    match (spec.min, spec.max) {
        (Some(lo), Some(hi)) => t_args(
            "settings.number.range_between",
            &[&spec.format(lo), &spec.format(hi), settled],
        ),
        (Some(lo), None) => t_args(
            "settings.number.range_at_least",
            &[&spec.format(lo), settled],
        ),
        (None, Some(hi)) => t_args(
            "settings.number.range_at_most",
            &[&spec.format(hi), settled],
        ),
        (None, None) => t_args("settings.number.range_snapped", &[settled]),
    }
}

/// `f32` 설정값을 [`number_field`] 로 편집한다. 확정된 프레임에만 `value` 가 바뀐다.
pub(super) fn number_field_f32(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: impl std::hash::Hash,
    spec: &NumberSpec<'_>,
    value: &mut f32,
) -> bool {
    let mut wide = f64::from(*value);
    let committed = number_field(ui, theme, id_salt, spec, &mut wide);
    if committed {
        *value = wide as f32;
    }
    committed
}

/// 숫자 칸 한 개를 그린다. `value` 는 확정된 값이고, 확정이 일어난 프레임에만 바뀐다.
///
/// 반환값은 **이 프레임에 확정이 일어났는가**다 — 호출처가 저장·재계산을 거는 자리다.
pub(super) fn number_field(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: impl std::hash::Hash,
    spec: &NumberSpec<'_>,
    value: &mut f64,
) -> bool {
    let shown = spec.format(*value);
    let mut changed = None;
    let committed = field(
        ui,
        theme,
        id_salt,
        spec,
        &shown,
        |text| out_of_range(text, spec).map(|v| spec.format(v)),
        |text| {
            if let Commit::Value(v) = commit(text, spec)
                && v != *value
            {
                changed = Some(v);
                return Some(spec.format(v));
            }
            None
        },
    );
    if let Some(v) = changed {
        *value = v;
    }
    committed
}

/// 정수 설정값 칸. 저장 형식(i64·u64·usize)을 잃지 않도록 `i128` 로 다루며, 읽은 값은 보정하지 않고
/// 그대로 보인다.
pub(super) fn integer_field(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: impl std::hash::Hash,
    spec: &NumberSpec<'_>,
    value: &mut i128,
) -> bool {
    let shown = value.to_string();
    let mut changed = None;
    let committed = field(
        ui,
        theme,
        id_salt,
        spec,
        &shown,
        |text| out_of_range_int(text, spec).map(|v| v.to_string()),
        |text| {
            let v = commit_int(text, spec).filter(|v| v != value)?;
            changed = Some(v);
            Some(v.to_string())
        },
    );
    if let Some(v) = changed {
        *value = v;
    }
    committed
}

/// 칸·단위·범위 줄을 그리고 blur·↵ 에서 `commit` 을 부른다. `commit` 은 값이 바뀌면 새 표기를 돌려준다.
fn field(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: impl std::hash::Hash,
    spec: &NumberSpec<'_>,
    shown: &str,
    pending: impl Fn(&str) -> Option<String>,
    mut commit: impl FnMut(&str) -> Option<String>,
) -> bool {
    // 프레임 간 유지되는 편집 버퍼. 포커스가 없는 동안에는 확정값의 표기로 되돌린다.
    let buf_id = egui::Id::new(("settings_number_buf", ui.id(), &id_salt));
    let mut buf = ui
        .data_mut(|d| d.get_temp::<String>(buf_id))
        .unwrap_or_else(|| shown.to_owned());

    // 저장 값을 그대로 둔 칸은 범위 밖이어도 경고하지 않고 확정하지도 않는다. 설정 파일에서 읽은 큰 값은
    // 사용자가 고쳐 확정할 때만 범위로 맞춘다.
    let pending = (buf != shown).then(|| pending(&buf)).flatten();
    let mut committed = false;

    ui.vertical(|ui| {
        if spec.range_note.is_some() {
            // 시안 숫자 칸은 칸 줄과 범위 문장 사이를 space-xs 로 둔다.
            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        }
        // 남은 폭에 단위가 들어가지 않으면 단위를 입력 칸 아래 줄로 내린다(단축키 행의 버튼 줄과 같은 감싸기).
        ui.horizontal_wrapped(|ui| {
            // 정수 칸은 값 전체가 보이도록 자란다. 편집 중에는 지금 친 글자를 센다.
            let width = if spec.decimals == 0 {
                tasty_ui_widgets::number_field_width(ui, theme, &buf)
            } else {
                theme.field_width_xs
            };
            let resp = tasty_ui_widgets::Input::new()
                .mono(true)
                .align(spec.align)
                .width(width.value())
                .enabled(spec.enabled)
                .invalid(pending.is_some())
                .show(ui, theme, &mut buf);

            if resp.lost_focus() && buf != shown {
                // 확정 — blur 와 `↵` 둘 다 여기로 온다.
                match commit(&buf) {
                    Some(next) => {
                        buf = next;
                        committed = true;
                    }
                    None => buf = shown.to_owned(),
                }
            } else if !resp.has_focus() && buf != shown {
                // 편집 중이 아니면 바깥에서 바뀐 값을 그대로 비춘다.
                buf = shown.to_owned();
            }

            if let Some(sfx) = spec.suffix {
                let ink = if spec.enabled {
                    theme.text_muted()
                } else {
                    theme.text_disabled()
                };
                // 단위와 범위 경고 줄 모두 caption 글꼴 크기다.
                let mut text = egui::RichText::new(sfx)
                    .size(theme.font_size_caption.value())
                    .color(ink);
                if spec.suffix_label {
                    text = text.family(tasty_ui_widgets::label_font_family());
                }
                ui.label(text);
            }

            let no_limit = spec
                .no_limit
                .filter(|(v, _)| buf.trim().parse::<i128>().ok() == Some(*v));
            match (no_limit, spec.unit) {
                (Some((_, label)), _) => {
                    tasty_ui_widgets::number_field_unit(ui, theme, label, false, spec.enabled)
                }
                (None, Some(unit)) => {
                    tasty_ui_widgets::number_field_unit(ui, theme, unit, true, spec.enabled)
                }
                (None, None) => {}
            }
        });

        if let Some(settled) = pending {
            match spec.range_note {
                Some(note) => {
                    // A caller may translate the range sentence with three positional slots.
                    // Static notes without slots retain their original text.
                    let note = translated_range_note(note, spec, &settled);
                    tasty_ui_widgets::number_field_note(ui, theme, &note);
                }
                None => {
                    ui.label(
                        egui::RichText::new(range_line(spec, &settled))
                            .size(theme.font_size_caption.value())
                            .color(theme.accent_danger_raised()),
                    );
                }
            }
        }
    });

    ui.data_mut(|d| d.insert_temp(buf_id, buf));
    committed
}

fn translated_range_note(note: &str, spec: &NumberSpec<'_>, settled: &str) -> String {
    let lo = spec.min.map(|v| spec.format(v)).unwrap_or_default();
    let hi = spec.max.map(|v| spec.format(v)).unwrap_or_default();
    tasty_i18n::fill_args(note, &[&lo, &hi, settled])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_notes_name_the_committed_bound_and_keep_static_notes() {
        let spec = NumberSpec::int(2.0, 10000.0);
        let settled = out_of_range_int("1", &spec).unwrap();
        assert_eq!(settled, 2);
        assert_eq!(
            translated_range_note(
                "Between {} and {}. Commits as {}.",
                &spec,
                &settled.to_string()
            ),
            "Between 2 and 10000. Commits as 2."
        );
        assert_eq!(
            translated_range_note("{2} within {0}–{1}", &spec, "2"),
            "2 within 2–10000"
        );
        assert_eq!(
            translated_range_note("Fixed note", &spec, "2"),
            "Fixed note"
        );
    }

    fn spec() -> NumberSpec<'static> {
        NumberSpec::int(25.0, 200.0)
    }

    #[test]
    fn a_value_inside_the_range_commits_as_typed() {
        assert_eq!(commit("150", &spec()), Commit::Value(150.0));
    }

    #[test]
    fn a_value_past_the_edge_commits_as_the_edge() {
        assert_eq!(commit("420", &spec()), Commit::Value(200.0));
        assert_eq!(commit("1", &spec()), Commit::Value(25.0));
    }

    #[test]
    fn a_half_typed_value_is_not_a_number_and_keeps_the_last_one() {
        // 치는 동안의 중간 상태 — 확정이 여기서 일어나지 않으므로 값이 안 끌려간다.
        assert_eq!(commit("", &spec()), Commit::Unparsed);
        assert_eq!(commit("-", &spec()), Commit::Unparsed);
        assert_eq!(commit("1e", &spec()), Commit::Unparsed);
        assert_eq!(commit("nan", &spec()), Commit::Unparsed);
    }

    #[test]
    fn the_step_snaps_before_the_range_clips() {
        // 1.0~10.0 · 눈금 0.5. 10.3 은 10.5 로 스냅됐다가 위 끝으로 잘린다 — 스냅이
        // 범위 밖으로 내보낼 수 있으므로 clamp 가 뒤에 와야 한다.
        let s = NumberSpec::int(1.0, 10.0).step(0.5).decimals(1);
        assert_eq!(commit("10.3", &s), Commit::Value(10.0));
        assert_eq!(commit("2.3", &s), Commit::Value(2.5));
        assert_eq!(commit("2.2", &s), Commit::Value(2.0));
    }

    #[test]
    fn the_warning_line_appears_only_when_the_value_would_be_pulled() {
        let s = spec();
        assert_eq!(out_of_range("150", &s), None);
        assert_eq!(out_of_range("420", &s), Some(200.0));
        // 숫자로 안 읽히는 중간 상태는 경고가 아니다 — 아직 아무것도 확정 안 됐다.
        assert_eq!(out_of_range("", &s), None);
    }

    #[test]
    fn a_range_with_no_edges_never_warns() {
        let s = NumberSpec {
            min: None,
            max: None,
            step: None,
            decimals: 0,
            suffix: None,
            suffix_label: false,
            unit: None,
            no_limit: None,
            range_note: None,
            align: egui::Align::RIGHT,
            enabled: true,
        };
        assert_eq!(commit("99999", &s), Commit::Value(99999.0));
        assert_eq!(out_of_range("99999", &s), None);
    }

    /// 입력 칸 뒤에 단위가 들어갈 자리가 없으면 단위는 아래 줄로 내려가고 영역 밖으로 나가지 않는다.
    #[test]
    fn the_suffix_wraps_under_the_field_in_a_narrow_row() {
        let th = crate::theme::theme();
        let width = th.field_width_xs.value() + th.spacing_sm.value();
        let spec = NumberSpec::int(1.0, 10_000.0).suffix("MiB");
        let ctx = egui::Context::default();
        tasty_egui_theme::apply_theme_to_egui(&th, &ctx);
        let mut used = egui::Rect::NOTHING;
        // 출력은 쓰지 않는다. 측정은 프레임 안에서 끝난다.
        drop(ctx.run(Default::default(), |ctx| {
            egui::Area::new(egui::Id::new("host"))
                .fixed_pos(egui::Pos2::ZERO)
                .show(ctx, |ui| {
                    ui.set_width(width);
                    ui.set_max_width(width);
                    let mut value = 500.0;
                    number_field(ui, &th, "narrow", &spec, &mut value);
                    used = ui.min_rect();
                });
        }));
        assert!(
            used.right() <= width + 0.5,
            "숫자 칸이 영역 폭 {width} 을 넘었다: {used:?}"
        );
        assert!(
            used.height() > th.settings_row_min_height().value(),
            "단위가 아래 줄로 내려가지 않았다: {used:?}"
        );
    }

    /// 성능 탭 디스크 보관 줄 수와 같은 칸.
    fn lines_spec() -> NumberSpec<'static> {
        NumberSpec::int(-1.0, 100_000_000.0)
            .align(egui::Align::LEFT)
            .unit("lines")
            .no_limit(-1, "no limit")
            .range_note("range")
    }

    /// 한 프레임에 그려진 글자와 테두리 상자.
    struct Shot {
        texts: Vec<(String, egui::Rect)>,
        boxes: Vec<(egui::Rect, egui::Color32)>,
        committed: bool,
    }

    impl Shot {
        fn text(&self, s: &str) -> Option<egui::Rect> {
            self.texts.iter().find(|(t, _)| t == s).map(|(_, r)| *r)
        }

        /// 칸 상자(포커스 링보다 작은, 테두리를 그린 사각형).
        fn field(&self) -> (egui::Rect, egui::Color32) {
            *self
                .boxes
                .iter()
                .min_by(|a, b| a.0.width().total_cmp(&b.0.width()))
                .expect("the field draws its edge")
        }
    }

    fn shot(
        ctx: &egui::Context,
        th: &Theme,
        spec: &NumberSpec<'_>,
        value: &mut i128,
        events: Vec<egui::Event>,
    ) -> Shot {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 300.0),
            )),
            events,
            ..Default::default()
        };
        let mut committed = false;
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                committed = integer_field(ui, th, "probe", spec, value);
            });
        });
        let mut texts = Vec::new();
        let mut boxes = Vec::new();
        for c in &out.shapes {
            match &c.shape {
                egui::Shape::Text(t) => texts.push((
                    t.galley.text().to_owned(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()),
                )),
                egui::Shape::Rect(r) if r.stroke.width > 0.0 => {
                    boxes.push((r.rect, r.stroke.color))
                }
                _ => {}
            }
        }
        Shot {
            texts,
            boxes,
            committed,
        }
    }

    fn click(pos: egui::Pos2) -> [Vec<egui::Event>; 2] {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        [
            vec![egui::Event::PointerMoved(pos), button(true)],
            vec![button(false)],
        ]
    }

    fn setup() -> (egui::Context, Theme) {
        let th = crate::theme::theme().clone();
        let ctx = egui::Context::default();
        tasty_egui_theme::apply_theme_to_egui(&th, &ctx);
        // 앱과 같은 D2Coding 으로 글자 폭을 잰다.
        crate::gfx::gpu::GpuState::setup_egui_fonts(&ctx);
        (ctx, th)
    }

    /// 글자가 칸의 왼쪽 padding 에서 시작해 오른쪽 padding 안에서 끝난다 — 잘리거나 안쪽으로 밀리지 않았다.
    fn assert_whole(th: &Theme, field: egui::Rect, text: egui::Rect) {
        let pad = th.input_padding_x().value();
        assert!(
            (text.left() - (field.left() + pad)).abs() < 0.5,
            "the value starts at the padding: {text:?} in {field:?}"
        );
        assert!(
            text.right() <= field.right() - pad + 0.5,
            "the whole value fits: {text:?} in {field:?}"
        );
    }

    /// 설정 파일의 가장 긴 i64 값은 f64 를 거치지 않고 19자리 그대로, 칸 안에 모두 보인다. 읽은 값이므로
    /// 범위 밖이어도 경고·보정이 없다.
    #[test]
    fn the_longest_config_value_shows_whole_and_unclamped() {
        let (ctx, th) = setup();
        let mut value = i128::from(i64::MAX);
        let spec = lines_spec();
        shot(&ctx, &th, &spec, &mut value, Vec::new());
        let at_rest = shot(&ctx, &th, &spec, &mut value, Vec::new());
        let text = at_rest
            .text("9223372036854775807")
            .expect("the value is drawn as written");
        let (field, edge) = at_rest.field();
        assert_whole(&th, field, text);
        assert_ne!(edge, th.input_border_invalid().to_egui());
        assert!(at_rest.text("range").is_none());
        assert!(at_rest.text("lines").is_some());

        // 고치지 않고 포커스만 오갔다면 확정하지 않는다.
        for events in click(field.center()) {
            shot(&ctx, &th, &spec, &mut value, events);
        }
        for events in click(egui::pos2(700.0, 250.0)) {
            assert!(!shot(&ctx, &th, &spec, &mut value, events).committed);
        }
        assert_eq!(value, i128::from(i64::MAX));
    }

    /// 편집 중에는 친 글자마다 칸이 자라 안쪽 스크롤이 없다. 범위 밖 입력은 invalid 테두리와 범위 줄을
    /// 보이고, 확정하면 범위 끝으로 맞춘다. -1 은 단위 대신 "no limit" 이다.
    #[test]
    fn editing_grows_the_field_per_char_and_commit_clamps() {
        let (ctx, th) = setup();
        let mut value = 10_000;
        let spec = lines_spec();
        shot(&ctx, &th, &spec, &mut value, Vec::new());
        let rest = shot(&ctx, &th, &spec, &mut value, Vec::new());
        let (field, _) = rest.field();
        assert_eq!(field.width(), th.number_field_min_width().value());
        for events in click(field.center()) {
            shot(&ctx, &th, &spec, &mut value, events);
        }
        let mut typed = String::from("10000");
        let mut last = field.width();
        for digit in "0000000000000".chars() {
            typed.push(digit);
            let frame = shot(
                &ctx,
                &th,
                &spec,
                &mut value,
                vec![egui::Event::Text(digit.to_string())],
            );
            let (field, _) = frame.field();
            assert_whole(&th, field, frame.text(&typed).expect("typed text"));
            let grown = shot(&ctx, &th, &spec, &mut value, Vec::new());
            let (field, edge) = grown.field();
            assert_whole(&th, field, grown.text(&typed).expect("typed text"));
            assert!(field.width() >= last);
            last = field.width();
            let over = typed.parse::<i128>().is_ok_and(|v| v > 100_000_000);
            assert_eq!(edge == th.input_border_invalid().to_egui(), over, "{typed}");
            assert_eq!(grown.text("range").is_some(), over, "{typed}");
        }
        assert!(last > th.number_field_min_width().value());
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(shot(&ctx, &th, &spec, &mut value, vec![enter]).committed);
        assert_eq!(value, 100_000_000);

        let mut unlimited = -1;
        shot(&ctx, &th, &spec, &mut unlimited, Vec::new());
        let frame = shot(&ctx, &th, &spec, &mut unlimited, Vec::new());
        assert!(frame.text("no limit").is_some());
        assert!(frame.text("lines").is_none());
    }

    #[test]
    fn an_integer_commit_keeps_every_digit() {
        let s = lines_spec();
        assert_eq!(commit_int("250000000", &s), Some(100_000_000));
        assert_eq!(commit_int("-5", &s), Some(-1));
        assert_eq!(
            out_of_range_int("9223372036854775807", &s),
            Some(100_000_000)
        );
        let open = NumberSpec {
            max: None,
            ..NumberSpec::int(1.0, 0.0)
        };
        assert_eq!(
            commit_int("9223372036854775807", &open),
            Some(i128::from(i64::MAX))
        );
        assert_eq!(commit_int("abc", &s), None);
    }

    #[test]
    fn the_display_keeps_the_declared_decimals() {
        assert_eq!(spec().format(150.0), "150");
        assert_eq!(NumberSpec::int(1.0, 10.0).decimals(1).format(2.0), "2.0");
        assert_eq!(NumberSpec::int(0.8, 2.0).decimals(2).format(1.2), "1.20");
    }
}
