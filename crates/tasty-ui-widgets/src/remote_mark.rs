//! 원격 위치 표지 — remote 글리프 + 호스트 글자. 핸들러 picker 경로 줄과 링크 툴팁이 쓴다.
//! 호스트 글자는 mono 로 상한에서 끝을 `…` 로 줄인다. 비었을 때 그 자리에 쓰는 낱말은 font-label 이고 줄이지 않는다.
//! 상태바 원격 항목은 바의 축소 규칙을 따르므로 `status_bar` 에 따로 있다.

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// 표지에 들어갈 글자. `host` 가 비면 `empty_label` 을 쓴다.
#[derive(Clone, Copy, Debug)]
pub struct RemoteMark<'a> {
    pub host: &'a str,
    pub empty_label: &'a str,
    /// 연결이 끊겨 다시 붙는 중이면 호스트 뒤에 붙는 상태 단어. 있으면 원격 색을 빼고 중립 잉크로
    /// 그린다(상태바의 재연결 형태). None 이면 평소 표지다.
    pub reconnecting: Option<&'a str>,
}

/// 표지 한 벌의 색·간격·상한.
#[derive(Clone, Copy)]
struct MarkStyle {
    glyph: egui::Color32,
    host: egui::Color32,
    word: egui::Color32,
    gap: LogicalPx,
    /// 호스트 글자 상한(글리프와 gap 은 밖).
    max_w: LogicalPx,
}

fn host_galley(
    ctx: &egui::Context,
    th: &Theme,
    mark: &RemoteMark<'_>,
    max_w: LogicalPx,
) -> Arc<egui::Galley> {
    let caption = th.font_size_caption.value();
    if mark.host.is_empty() {
        // 빈 호스트 자리의 낱말은 상한을 받지 않고 끝까지 그린다(정본 'no host' 행).
        return label_galley(ctx, th, mark.empty_label);
    }
    let mut job = egui::text::LayoutJob::simple_singleline(
        mark.host.to_owned(),
        egui::FontId::monospace(caption),
        egui::Color32::PLACEHOLDER,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: max_w.max(LogicalPx(0.0)).value(),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ctx.fonts(|f| f.layout_job(job))
}

/// font-label caption 한 줄. 줄이지 않는다.
fn label_galley(ctx: &egui::Context, th: &Theme, text: &str) -> Arc<egui::Galley> {
    ctx.fonts(|f| {
        f.layout_no_wrap(
            text.to_owned(),
            crate::label_font(th.font_size_caption.value()),
            egui::Color32::PLACEHOLDER,
        )
    })
}

/// 잰 표지 — 호스트 galley 와 그 자리 폭, 재연결 상태 단어, 전체 폭.
struct Laid {
    host: Arc<egui::Galley>,
    host_w: LogicalPx,
    word: Option<Arc<egui::Galley>>,
    width: LogicalPx,
}

/// 글리프 + gap + 호스트 (+ gap + 상태 단어). 호스트 글자는 `max_w` 상한에서 끝을 줄이고, 줄였으면 상한
/// 폭을 차지한다. 상태 단어는 줄이지 않는다.
fn layout(ui: &egui::Ui, th: &Theme, mark: &RemoteMark<'_>, style: MarkStyle) -> Laid {
    let host = host_galley(ui.ctx(), th, mark, style.max_w);
    // 상한에서 줄인 호스트는 상한 폭을 차지한다(정본: 경로 상자 = 390 − (12 + 4 + 160) − 8).
    let host_w = if host.elided {
        style.max_w
    } else {
        LogicalPx(host.rect.width())
    };
    let word = mark.reconnecting.map(|w| label_galley(ui.ctx(), th, w));
    let word_w = word
        .as_ref()
        .map_or(LogicalPx(0.0), |g| style.gap + LogicalPx(g.rect.width()));
    let width = th.icon_glyph_size_xs + style.gap + host_w + word_w;
    Laid {
        host,
        host_w,
        word,
        width,
    }
}

fn paint(ui: &egui::Ui, th: &Theme, left_center: egui::Pos2, laid: Laid, style: MarkStyle) {
    let gs = th.icon_glyph_size_xs.value();
    let grect = egui::Rect::from_center_size(
        egui::pos2(left_center.x + gs / 2.0, left_center.y),
        egui::Vec2::splat(gs),
    );
    tasty_icons::REMOTE
        .image(gs, style.glyph)
        .paint_at(ui, grect);
    let host_left = left_center.x + gs + style.gap.value();
    let pos = egui::pos2(host_left, left_center.y - laid.host.rect.height() / 2.0);
    ui.painter().galley(pos, laid.host, style.host);
    if let Some(word) = laid.word {
        let pos = egui::pos2(
            host_left + (laid.host_w + style.gap).value(),
            left_center.y - word.rect.height() / 2.0,
        );
        ui.painter().galley(pos, word, style.word);
    }
}

/// 핸들러 picker 의 표지 색. 재연결 중이면 글리프는 glyph-dim, 호스트와 상태 단어는 text-muted 다.
fn picker_style(th: &Theme, reconnecting: bool) -> MarkStyle {
    let (glyph, host) = if reconnecting {
        (th.glyph_dim().into(), th.text_muted().into())
    } else {
        let remote: egui::Color32 = th.fh_remote_fg().into();
        (remote, remote)
    };
    MarkStyle {
        glyph,
        host,
        word: th.text_muted().into(),
        gap: th.fh_remote_gap(),
        max_w: th.fh_remote_max_width(),
    }
}

/// 핸들러 picker 경로 줄 앞 표지의 폭(글리프 + gap + 호스트, 호스트는 `fh-remote-max-width` 상한,
/// 재연결 중이면 + gap + 상태 단어).
pub fn fh_remote_mark_width(ui: &egui::Ui, th: &Theme, mark: &RemoteMark<'_>) -> LogicalPx {
    layout(ui, th, mark, picker_style(th, mark.reconnecting.is_some())).width
}

/// 핸들러 picker 표지의 재연결 상태 단어 폭(font-label caption, 줄이지 않음). 경로 글자 폭을 잴 수 없어도
/// 이 값이 유효하면 경로 상자 추정에 그대로 쓴다([`UnmeasuredMark::word_width`](crate::file_handler::UnmeasuredMark::word_width)).
pub fn fh_remote_word_width(ui: &egui::Ui, th: &Theme, word: &str) -> LogicalPx {
    LogicalPx(label_galley(ui.ctx(), th, word).rect.width())
}

/// 핸들러 picker 원격 표지 옆 경로의 글자 수. 표지 폭과 재연결 상태 단어 폭은 여기서 재고, 경로 글자 폭
/// `advance`("00"과 "0"의 폭 차이)는 호출 쪽이 잰다. 본체와 갤러리가 같이 쓴다.
pub fn fh_remote_path_budget(
    ui: &egui::Ui,
    th: &Theme,
    mark: &RemoteMark<'_>,
    advance: LogicalPx,
    language: &str,
) -> usize {
    let word_w = mark
        .reconnecting
        .map(|word| fh_remote_word_width(ui, th, word));
    fh_remote_path_budget_from(
        th,
        advance,
        fh_remote_mark_width(ui, th, mark),
        word_w,
        mark,
        language,
    )
}

/// 잰 값으로 정하는 원격 표지 옆 경로의 글자 수. 경로 상자는 표지와 그 뒤 간격(space-sm)을 뺀 나머지다.
/// 잰 값(`advance`·`mark_w`)이 유효하지 않으면 Theme 토큰과 `language`로 추정하고, 상태 단어는 잰 폭
/// `word_w`가 유효하면 그 값을 쓴다.
pub fn fh_remote_path_budget_from(
    th: &Theme,
    advance: LogicalPx,
    mark_w: LogicalPx,
    word_w: Option<LogicalPx>,
    mark: &RemoteMark<'_>,
    language: &str,
) -> usize {
    crate::file_handler::target_budget_chars_beside(
        advance,
        mark_w,
        th.spacing_sm,
        &crate::file_handler::UnmeasuredMark {
            host: mark.host,
            reconnecting: mark.reconnecting,
            word_width: word_w,
            language,
            glyph: th.icon_glyph_size_xs,
            mark_gap: th.fh_remote_gap(),
            host_cap: th.fh_remote_max_width(),
            em: crate::tokens::FH_STATUS_WORD_EM_UNMEASURED,
        },
    )
}

/// 핸들러 picker 경로 줄 앞 표지를 `left_center`(왼쪽 끝, 세로 가운데)에 그리고 폭을 돌려준다.
pub fn paint_fh_remote_mark(
    ui: &egui::Ui,
    th: &Theme,
    left_center: egui::Pos2,
    mark: &RemoteMark<'_>,
) -> LogicalPx {
    let style = picker_style(th, mark.reconnecting.is_some());
    let laid = layout(ui, th, mark, style);
    let w = laid.width;
    paint(ui, th, left_center, laid, style);
    w
}

/// 링크 툴팁의 내용 — `Remote` 낱말(tooltip-fg) · space-sm · 글리프 + 호스트(link-remote-fg).
/// 호스트가 비면 낱말과 글리프만 그린다(낱말을 두 번 쓰지 않는다). 전체가 `tooltip-max-width`
/// 안에 들도록 호스트 끝을 줄인다.
pub struct LinkRemoteTip<'a> {
    pub word: &'a str,
    pub host: &'a str,
}

impl LinkRemoteTip<'_> {
    fn word_galley(&self, ctx: &egui::Context, th: &Theme) -> Arc<egui::Galley> {
        ctx.fonts(|f| {
            f.layout_no_wrap(
                self.word.to_owned(),
                egui::FontId::proportional(th.font_size_caption.value()),
                egui::Color32::PLACEHOLDER,
            )
        })
    }

    fn style(th: &Theme) -> MarkStyle {
        let fg: egui::Color32 = th.link_remote_fg().into();
        MarkStyle {
            glyph: fg,
            host: fg,
            word: fg,
            gap: th.link_remote_gap(),
            // 링크 툴팁은 따로 상한이 없고 툴팁 최대 폭 안에서 줄인다.
            max_w: th.tooltip_max_width,
        }
    }

    /// 버블 안쪽(여백 제외)의 내용 폭과 높이.
    fn content(
        &self,
        ctx: &egui::Context,
        th: &Theme,
    ) -> (Arc<egui::Galley>, Option<Arc<egui::Galley>>, egui::Vec2) {
        let word = self.word_galley(ctx, th);
        let inner_max = th.tooltip_max_width - th.spacing_sm.scaled(2.0);
        let before = LogicalPx(word.rect.width()) + th.spacing_sm;
        let style = Self::style(th);
        let glyph = th.icon_glyph_size_xs;
        let host = (!self.host.is_empty()).then(|| {
            let room = inner_max - before - glyph - style.gap;
            host_galley(
                ctx,
                th,
                &RemoteMark {
                    host: self.host,
                    empty_label: "",
                    reconnecting: None,
                },
                room,
            )
        });
        let host_w = host
            .as_ref()
            .map_or(LogicalPx(0.0), |g| style.gap + LogicalPx(g.rect.width()));
        let h = LogicalPx(word.rect.height()).max(glyph).max(
            host.as_ref()
                .map_or(LogicalPx(0.0), |g| LogicalPx(g.rect.height())),
        );
        let w = before + glyph + host_w;
        (word, host, egui::vec2(w.value(), h.value()))
    }

    /// 버블 크기 — 내용 + 여백(space-sm · space-xs) + 테두리.
    pub fn bubble_size(&self, ctx: &egui::Context, th: &Theme) -> egui::Vec2 {
        let (_, _, inner) = self.content(ctx, th);
        let border = th.border_width.scaled(2.0).value();
        inner
            + egui::vec2(
                th.spacing_sm.scaled(2.0).value() + border,
                th.spacing_xs.scaled(2.0).value() + border,
            )
    }

    /// `rect`(버블 바깥 사각형)에 `ui` 의 painter 로 버블을 그린다. 갤러리처럼 Area 없이 그리는 곳도
    /// 이 함수를 쓴다.
    pub fn paint(&self, ui: &egui::Ui, th: &Theme, rect: egui::Rect) {
        let ctx = ui.ctx();
        let painter = ui.painter();
        let (word, host, inner) = self.content(ctx, th);
        let style = Self::style(th);
        let radius = th.corner_radius.value();
        painter.add(th.shadow_popover().to_egui().as_shape(rect, radius));
        painter.rect(
            rect,
            radius,
            th.surface_raised().to_egui(),
            egui::Stroke::new(th.border_width.value(), th.border_strong().to_egui()),
            egui::StrokeKind::Inside,
        );
        let pad = egui::vec2(th.spacing_sm.value(), th.spacing_xs.value());
        let r = egui::Rect::from_min_size(
            rect.min + pad + egui::Vec2::splat(th.border_width.value()),
            inner,
        );
        let cy = r.center().y;
        let word_w = word.rect.width();
        let word_h = word.rect.height();
        painter.galley(
            egui::pos2(r.left(), cy - word_h / 2.0),
            word,
            th.tooltip_fg().into(),
        );
        let left = r.left() + word_w + th.spacing_sm.value();
        let gs = th.icon_glyph_size_xs.value();
        let grect =
            egui::Rect::from_center_size(egui::pos2(left + gs / 2.0, cy), egui::Vec2::splat(gs));
        tasty_icons::REMOTE
            .image(gs, style.glyph)
            .paint_at(ui, grect);
        if let Some(host) = host {
            let h = host.rect.height();
            painter.galley(
                egui::pos2(left + gs + style.gap.value(), cy - h / 2.0),
                host,
                style.host,
            );
        }
    }

    /// `anchor`(링크 런) 아래 툴팁 층에 버블을 그린다. 아래에 자리가 없으면 위, 가로는 `bounds` 안으로 당긴다.
    pub fn show(&self, ctx: &egui::Context, th: &Theme, anchor: egui::Rect, bounds: egui::Rect) {
        let size = self.bubble_size(ctx, th);
        let rect = link_tip_rect(anchor, size, th.tooltip_offset(), bounds);
        egui::Area::new(egui::Id::new("tasty_link_remote_tip"))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
                self.paint(ui, th, r);
            });
    }
}

/// 링크 툴팁 자리 — 링크 아래, 아래에 자리가 없으면 위. 가로는 링크 가운데에서 `bounds` 안으로 당긴다.
pub fn link_tip_rect(
    anchor: egui::Rect,
    size: egui::Vec2,
    offset: LogicalPx,
    bounds: egui::Rect,
) -> egui::Rect {
    let offset = offset.value();
    let below = anchor.bottom() + offset;
    let top = if below + size.y <= bounds.bottom() {
        below
    } else {
        (anchor.top() - offset - size.y).max(bounds.top())
    };
    let left = (anchor.center().x - size.x / 2.0)
        .min(bounds.right() - size.x)
        .max(bounds.left());
    egui::Rect::from_min_size(egui::pos2(left, top), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    /// 앱 글꼴(D2Coding mono)을 깐 한 프레임 안에서 `f` 를 부른다. 폭 계산에 글꼴이 필요하다.
    fn in_frame<R>(f: impl FnOnce(&egui::Ui) -> R) -> R {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "d2coding".to_owned(),
            Arc::new(egui::FontData::from_static(
                tasty_font::D2CODING_REGULAR_TTF,
            )),
        );
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "d2coding".to_owned());
        ctx.set_fonts(fonts);
        let mut f = Some(f);
        let mut out = None;
        // 그린 도형은 보지 않고 프레임 안에서 잰 값만 쓴다.
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(f) = f.take() {
                    out = Some(f(ui));
                }
            });
        });
        out.expect("frame ran")
    }

    /// 빈 호스트 자리의 낱말은 호스트 상한(160)으로 줄이지 않는다(정본 'no host' 행 "never elides").
    #[test]
    fn the_no_host_word_is_never_cut_by_the_host_cap() {
        let th = theme();
        let word = "Remote".repeat(8);
        let (w, cap) = in_frame(|ui| {
            let mark = RemoteMark {
                host: "",
                empty_label: &word,
                reconnecting: None,
            };
            (
                fh_remote_mark_width(ui, &th, &mark),
                th.fh_remote_max_width(),
            )
        });
        let outside = th.icon_glyph_size_xs + th.fh_remote_gap();
        assert!(
            w > outside + cap,
            "the word keeps its full width: mark {w:?}, cap {cap:?}"
        );
    }

    /// 상한에서 줄인 호스트는 상한 폭을 차지한다. 경로 상자는 390 − (12 + 4 + 160) − 8 이다
    /// (정본 'path box' 행, 상한 표본 206px).
    #[test]
    fn a_host_cut_at_the_cap_takes_the_whole_cap() {
        let th = theme();
        let w = in_frame(|ui| {
            let mark = RemoteMark {
                host: "build-runner-eu-west-07-staging-attach-with-a-longer-name",
                empty_label: "Remote",
                reconnecting: None,
            };
            fh_remote_mark_width(ui, &th, &mark)
        });
        assert_eq!(
            w,
            th.icon_glyph_size_xs + th.fh_remote_gap() + th.fh_remote_max_width()
        );
    }

    /// 재연결 중이면 상태 단어가 gap 뒤에 붙어 표지가 그만큼 넓어진다. 긴 호스트(상한)와 빈 호스트 모두
    /// 단어를 줄이지 않는다.
    #[test]
    fn the_reconnecting_word_follows_the_host_and_widens_the_mark() {
        let th = theme();
        let word = "reconnecting";
        for host in ["host", "build-runner-eu-west-07-staging-attach", ""] {
            let (live, lost, word_w) = in_frame(|ui| {
                let mark = |reconnecting| RemoteMark {
                    host,
                    empty_label: "Remote",
                    reconnecting,
                };
                let word_w = label_galley(ui.ctx(), &th, word).rect.width();
                (
                    fh_remote_mark_width(ui, &th, &mark(None)),
                    fh_remote_mark_width(ui, &th, &mark(Some(word))),
                    LogicalPx(word_w),
                )
            });
            assert_eq!(lost, live + th.fh_remote_gap() + word_w, "host {host:?}");
        }
    }

    /// 그린 표지의 글리프 tint 와 글자 색들.
    fn painted_inks(mark: RemoteMark<'_>) -> (Vec<egui::Color32>, Vec<egui::Color32>) {
        let th = theme();
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                paint_fh_remote_mark(ui, &th, egui::pos2(20.0, 20.0), &mark);
            });
        });
        let mut glyphs = Vec::new();
        let mut texts = Vec::new();
        for clipped in out.shapes {
            match clipped.shape {
                egui::Shape::Rect(r) if r.brush.is_some() => glyphs.push(r.fill),
                egui::Shape::Text(t) => texts.push(t.fallback_color),
                _ => {}
            }
        }
        (glyphs, texts)
    }

    /// 평소 표지는 글리프와 호스트 모두 fh-remote-fg, 재연결 중이면 글리프 glyph-dim · 호스트와 상태 단어
    /// text-muted 다(정본 Meta 'reconnecting' 행).
    #[test]
    fn a_reconnecting_mark_drops_to_neutral_ink() {
        let th = theme();
        let remote: egui::Color32 = th.fh_remote_fg().into();
        let (glyphs, texts) = painted_inks(RemoteMark {
            host: "work-laptop-attach",
            empty_label: "Remote",
            reconnecting: None,
        });
        assert_eq!(glyphs, vec![remote]);
        assert_eq!(texts, vec![remote]);

        let muted: egui::Color32 = th.text_muted().into();
        let (glyphs, texts) = painted_inks(RemoteMark {
            host: "work-laptop-attach",
            empty_label: "Remote",
            reconnecting: Some("reconnecting"),
        });
        assert_eq!(glyphs, vec![egui::Color32::from(th.glyph_dim())]);
        assert_eq!(texts, vec![muted, muted]);
    }

    /// 링크 툴팁을 버블 크기대로 한 프레임 그리고 (버블 폭, 칠한 글자와 글꼴 가족, 칠한 글리프 수)를 돌려준다.
    fn painted_tip(host: &str) -> (f32, Vec<(String, egui::FontFamily)>, usize) {
        let th = theme();
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let tip = LinkRemoteTip {
            word: "Remote",
            host,
        };
        let mut width = 0.0;
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let size = tip.bubble_size(ctx, &th);
                width = size.x;
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                tip.paint(ui, &th, rect);
            });
        });
        let mut texts = Vec::new();
        let mut glyphs = 0;
        for clipped in out.shapes {
            match clipped.shape {
                egui::Shape::Rect(r) if r.brush.is_some() => glyphs += 1,
                egui::Shape::Text(t) => texts.push((
                    t.galley.text().to_owned(),
                    t.galley.job.sections[0].format.font_id.family.clone(),
                )),
                _ => {}
            }
        }
        (width, texts, glyphs)
    }

    /// 프로필 이름이 없는 attach(빈 호스트)의 링크 툴팁은 머리 낱말 Remote와 글리프만 그린다. 호스트 글자와
    /// 글리프 뒤 간격이 없다(정본 Meta host 'no profile name → no host text, Remote then the glyph alone').
    /// 머리 낱말은 호스트 자리의 낱말이 아니라 늘 붙는 툴팁 머리라 UI 글꼴(proportional)이다.
    #[test]
    fn a_link_tip_with_no_host_draws_the_word_and_the_glyph_alone() {
        let th = theme();
        let (empty_w, texts, glyphs) = painted_tip("");
        assert_eq!(
            texts,
            vec![("Remote".to_owned(), egui::FontFamily::Proportional)]
        );
        assert_eq!(glyphs, 1);
        let word_w = in_frame(|ui| {
            ui.fonts(|f| {
                f.layout_no_wrap(
                    "Remote".to_owned(),
                    egui::FontId::proportional(th.font_size_caption.value()),
                    egui::Color32::PLACEHOLDER,
                )
            })
            .rect
            .width()
        });
        // 낱말 + space-sm + 글리프 + 좌우 여백 + 테두리 — 글리프 뒤 간격과 호스트 폭은 없다.
        let bare = word_w
            + th.spacing_sm.value()
            + th.icon_glyph_size_xs.value()
            + th.spacing_sm.scaled(2.0).value()
            + th.border_width.scaled(2.0).value();
        assert!((empty_w - bare).abs() < 0.01, "{empty_w} vs {bare}");

        // 이름이 있으면 글리프 뒤에 간격과 mono 호스트가 붙는다.
        let (named_w, texts, glyphs) = painted_tip("build-eu");
        assert_eq!(
            texts,
            vec![
                ("Remote".to_owned(), egui::FontFamily::Proportional),
                ("build-eu".to_owned(), egui::FontFamily::Monospace),
            ]
        );
        assert_eq!(glyphs, 1);
        assert!(named_w > empty_w + th.link_remote_gap().value());
    }

    fn bounds() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    #[test]
    fn the_link_tip_sits_below_the_run_when_there_is_room() {
        let anchor = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(84.0, 18.0));
        let r = link_tip_rect(anchor, egui::vec2(120.0, 24.0), LogicalPx(4.0), bounds());
        assert_eq!(r.top(), anchor.bottom() + 4.0);
        assert_eq!(r.center().x, anchor.center().x);
    }

    #[test]
    fn the_link_tip_goes_above_when_the_bottom_is_full_and_stays_in_the_window() {
        let anchor = egui::Rect::from_min_size(egui::pos2(780.0, 580.0), egui::vec2(20.0, 18.0));
        let r = link_tip_rect(anchor, egui::vec2(120.0, 24.0), LogicalPx(4.0), bounds());
        assert_eq!(r.bottom(), anchor.top() - 4.0);
        assert!(bounds().contains_rect(r));
    }
}
