//! 형식 이름과 목록의 링크 예제(시안 batch 11 "Kind words" · "Links in the listing").
//! Type 열 낱말은 본체와 같은 번역 문구로 만든다.

use super::*;

/// 상세 표 한 행. `link` 는 이름 뒤 link 글리프, `broken` 은 항목 자리의 warning link 글리프다.
struct KindRow {
    glyph: MockGlyph,
    image: bool,
    name: &'static str,
    link: bool,
    broken: bool,
    size: &'static str,
    date: &'static str,
    kind: String,
}

fn row(
    glyph: MockGlyph,
    name: &'static str,
    size: &'static str,
    date: &'static str,
    kind: String,
) -> KindRow {
    KindRow {
        glyph,
        image: false,
        name,
        link: false,
        broken: false,
        size,
        date,
        kind,
    }
}

fn kind_rows() -> Vec<KindRow> {
    let markdown = t("explorer.kind.markdown");
    vec![
        row(
            icons::FOLDER,
            "Documents",
            "—",
            "2026-10-02 10:14",
            t("explorer.type.folder").into(),
        ),
        row(
            icons::FILE,
            "backup-2026.tar.gz",
            "1.2 GB",
            "2026-09-30 22:01",
            t("explorer.kind.archive").into(),
        ),
        row(
            icons::FILE,
            "report.pdf",
            "2.4 MB",
            "2026-06-24 09:12",
            t("explorer.kind.pdf").into(),
        ),
        KindRow {
            image: true,
            ..row(
                icons::IMAGE,
                "diagram.png",
                "488 KB",
                "2026-06-26 18:05",
                t("explorer.kind.image").replace("{type}", "PNG"),
            )
        },
        row(
            icons::FILE,
            "notes.md",
            "12 KB",
            "2026-06-27 11:40",
            markdown.into(),
        ),
        KindRow {
            link: true,
            ..row(
                icons::FILE,
                "link.md",
                "6 B",
                "2026-10-01 08:00",
                t("explorer.kind.link_to").replace("{kind}", markdown),
            )
        },
        row(
            icons::FILE,
            "build.rs",
            "3 KB",
            "2026-10-03 16:20",
            t("explorer.kind.ext_file").replace("{ext}", "RS"),
        ),
    ]
}

fn link_rows() -> Vec<KindRow> {
    let link_to = t("explorer.kind.link_to");
    let mut rows = kind_rows();
    let pdf = rows.remove(2);
    let md_link = rows.remove(4);
    vec![
        KindRow {
            link: true,
            ..row(
                icons::FOLDER,
                "projects",
                "—",
                "2026-10-04 12:00",
                link_to.replace("{kind}", t("explorer.type.folder")),
            )
        },
        md_link,
        KindRow {
            broken: true,
            ..row(
                icons::LINK,
                "old-config.toml",
                "—",
                "2026-08-11 09:30",
                t("explorer.kind.broken_link").into(),
            )
        },
        pdf,
    ]
}

fn cell(ui: &mut egui::Ui, th: &Theme, r: &KindRow, col: usize) {
    let muted = th.text_muted().to_egui();
    let mono = egui::FontId::monospace(th.font_size_caption.value());
    match col {
        0 => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                let sz = th.icon_glyph_size_md.value();
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(sz), egui::Sense::hover());
                let tint = if r.broken {
                    th.explorer_link_broken_fg().to_egui()
                } else if r.image {
                    th.accent_info().to_egui()
                } else {
                    th.text_secondary().to_egui()
                };
                r.glyph.image(sz, tint).paint_at(ui, rect);
                ui.label(
                    egui::RichText::new(r.name)
                        .size(th.font_size_body.value())
                        .color(th.table_row_fg().to_egui()),
                );
                if r.link {
                    ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
                    let s = th.explorer_link_glyph_size().value();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(s), egui::Sense::hover());
                    icons::LINK
                        .image(s, th.explorer_link_glyph().to_egui())
                        .paint_at(ui, rect);
                }
            });
        }
        1 => {
            ui.add_space(th.spacing_sm.value());
            ui.label(egui::RichText::new(r.size).font(mono).color(muted));
        }
        2 => {
            ui.label(egui::RichText::new(r.date).font(mono).color(muted));
        }
        _ => {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&r.kind)
                        .size(th.font_size_caption.value())
                        .color(muted),
                )
                .truncate(),
            );
        }
    }
}

fn table(ui: &mut egui::Ui, theme: &Theme, rows: &[KindRow], id: &str) {
    framed(ui, theme, |ui| {
        ui.set_width(WIDE_W.value().min(ui.available_width()));
        detail_columns_table(ui, theme, rows, id, cell);
    });
}

/// Type 열 낱말과 링크 행의 미리보기 머리.
pub fn draw_kind_words(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Solo, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            lbl(ui, theme, "Detail — Type column words");
            table(ui, theme, &kind_rows(), "gallery_exp_kind_words");
            lbl(ui, theme, "preview header — link");
            let facts = format!(
                "{} · 6 B",
                t("explorer.kind.link_to").replace("{kind}", t("explorer.kind.markdown"))
            );
            preview_header(ui, theme, "link.md", &facts);
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "table",
                "md → Markdown · txt → Text · pdf → PDF document · archives → Archive · shell → Shell script · images → {TYPE} image",
            ),
            ("fallback", "{EXT} file · no extension File · Folder"),
            (
                "Type column",
                "same word · ellipsis · full word in the tooltip",
            ),
            (
                "link",
                "Link to {kind} · size = target · Properties Kind stays Symbolic link · broken Broken link",
            ),
            ("byte limit", "Over 1 MiB. (NBSP between number and unit)"),
        ],
        &[TokenChip::new(
            "text-muted",
            "Type column",
            theme.text_muted().to_egui(),
        )],
    );
}

/// 시안 미리보기 머리: 이름 body · "종류 · 크기" caption muted, 40 높이 · 288 폭.
fn preview_header(ui: &mut egui::Ui, theme: &Theme, name: &str, facts: &str) {
    let size = egui::vec2(
        theme.explorer_preview_width().value(),
        theme.explorer_preview_header_height().value(),
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        theme.bg_panel().to_egui(),
    );
    p.rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let name_g = p.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_primary().to_egui(),
    );
    let facts_g = p.layout_no_wrap(
        facts.to_owned(),
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
    let left = rect.left() + theme.spacing_md.value();
    let top = rect.center().y - (name_g.rect.height() + facts_g.rect.height()) / 2.0;
    let name_h = name_g.rect.height();
    p.galley(
        egui::pos2(left, top),
        name_g,
        theme.text_primary().to_egui(),
    );
    p.galley(
        egui::pos2(left, top + name_h),
        facts_g,
        theme.text_muted().to_egui(),
    );
}

/// 대상이 있는 링크 · 대상이 없는 링크.
pub fn draw_links(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Solo, |ui| {
        themes(ui, theme, |ui, th| {
            table(ui, th, &link_rows(), "gallery_exp_links")
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "link",
                "target glyph · trailing link 12 · gap space-xs · explorer-link-glyph",
            ),
            (
                "broken",
                "link glyph in the item slot · explorer-link-broken-fg → accent-warning · name normal · Type Broken link · tooltip Target not found: {path}",
            ),
            ("views", "Detail · List · Grid alike"),
        ],
        &[
            TokenChip::new(
                "explorer-link-glyph",
                "→ text-muted",
                theme.explorer_link_glyph().to_egui(),
            ),
            TokenChip::new(
                "explorer-link-broken-fg",
                "→ accent-warning",
                theme.explorer_link_broken_fg().to_egui(),
            ),
        ],
    );
}
