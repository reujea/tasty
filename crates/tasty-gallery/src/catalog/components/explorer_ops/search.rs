//! 검색 예제 — Find 바와 하위 폴더 검색. 부모 모듈의 예제 도우미를 함께 쓴다.

use super::*;

pub fn search_section() -> Section {
    Section {
        id: "explorer-search",
        title: "Explorer — filter · search",
        specs: vec![
            Spec {
                id: "explorer-find-bar",
                title: "Find bar — filter the current folder",
                when: Some(
                    "bar under the toolbar · substring, case-insensitive · “{shown} of {total}”",
                ),
                draw: draw_find_bar,
            },
            Spec {
                id: "explorer-subfolder-search",
                title: "Subfolders — searching · skipped · stopped · no results · error",
                when: Some("results stream in · Detail swaps Type for a relative Folder column"),
                draw: draw_subfolder_search,
            },
            // 형식 이름과 링크 표시는 목록 행의 일이라 이 절에 둔다(갤러리 목차는 동결 파일이다).
            Spec {
                id: "explorer-kind-words",
                title: "Kind words — one table for Type, Kind and the preview header",
                when: Some("format-name table · fallback {EXT} file · links say Link to {kind}"),
                draw: super::kinds::draw_kind_words,
            },
            Spec {
                id: "explorer-links",
                title: "Links in the listing · broken link",
                when: Some(
                    "target glyph + trailing link glyph · broken = warning link glyph in the item slot",
                ),
                draw: super::kinds::draw_links,
            },
        ],
    }
}

fn find_labels(placeholder: &str) -> ExplorerFindLabels<'_> {
    ExplorerFindLabels {
        placeholder,
        subfolders: t("explorer.find.subfolders"),
        close: t("explorer.find.close"),
    }
}

/// 시안 `XCell` — 툴바 · Find 바 · 본문 · 상태줄.
fn find_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    query: &str,
    deep: Option<bool>,
    status: ExplorerFindStatus<'_>,
    status_line: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    framed(ui, theme, |ui| {
        let w = SEARCH_W.value().min(ui.available_width());
        ui.set_width(w);
        toolbar(ui, theme, w, "~/Downloads", Some(true), true, false);
        let mut q = query.to_string();
        let mut d = deep.unwrap_or(false);
        let placeholder = if d {
            t_fmt("explorer.find.search_placeholder", "Downloads")
        } else {
            t("explorer.find.filter_placeholder").to_string()
        };
        let labels = find_labels(&placeholder);
        explorer_find_bar(
            ui,
            theme,
            ExplorerFindBar {
                query: &mut q,
                subfolders: deep.is_some().then_some(&mut d),
                status,
                labels,
                focus: false,
            },
        );
        body(ui);
        status_bar(ui, theme, status_line);
    });
}

pub(super) fn status_bar(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme.item_height_interactive.value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
    );
    ui.painter().text(
        egui::pos2(rect.left() + theme.spacing_md.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
}

pub fn draw_find_bar(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Solo, |ui| {
        themes(ui, theme, |ui, th| {
            find_cell(
                ui,
                th,
                "re",
                Some(false),
                ExplorerFindStatus::Text(&t_fmt2("explorer.find.count", "4", "7")),
                &t_fmt2("explorer.find.status", "4", "7"),
                |ui| {
                    let rows = [
                        (
                            icons::FILE,
                            "report.pdf",
                            "2.4 MB",
                            "2026-06-24 09:12",
                            "PDF document",
                        ),
                        (icons::FOLDER, "mockup-exports", "—", "2026-06-20 14:30", ""),
                        (
                            icons::FILE,
                            "cafe\u{301}-report.txt",
                            "1 KB",
                            "2026-06-24 09:12",
                            "Text file",
                        ),
                        (
                            icons::FILE,
                            "\u{1100}\u{1161}\u{1102}\u{1161}-report.txt",
                            "1 KB",
                            "2026-06-24 09:12",
                            "Text file",
                        ),
                    ];
                    detail_columns_table(ui, th, &rows, "gallery_exp_find", |ui, th, r, col| {
                        if col == 0 {
                            name_cell(ui, th, r.0, r.1, "re");
                        } else {
                            cell_text(ui, th, col, r.0, r.1, r.2, r.3, r.4);
                        }
                    });
                },
            );
        });
    });
    let unicode_note = format!(
        "NFC display ({}), raw substring matching; composed clusters keep their highlight together",
        tasty_ui_widgets::display_nfc("cafe\u{301}")
    );
    meta(
        ui,
        theme,
        &[
            (
                "placement",
                "bar under the toolbar · full cell width · explorer-search-bar-height 36",
            ),
            (
                "field",
                "Input + search icon · placeholder “Filter this folder”",
            ),
            (
                "match",
                "substring, case-insensitive · matched part in accent-primary-raised",
            ),
            ("Unicode", unicode_note.as_str()),
            ("count", "mono caption · “{shown} of {total}”"),
            (
                "type-ahead",
                "off in the field · on the list it cycles visible rows",
            ),
            ("Esc", "clear → close"),
            ("closes on", "× · Back · Up · address bar · folder change"),
            ("remote", "filter yes"),
        ],
        &[
            TokenChip::without_color("explorer-search-bar-height", "→ size-36"),
            TokenChip::new(
                "accent-primary-raised",
                "match",
                theme.accent_primary_raised().to_egui(),
            ),
            TokenChip::new("input-bg", "field", theme.input_bg().to_egui()),
        ],
    );
}

/// 글리프와 찾은 부분을 칠한 이름.
fn name_cell(ui: &mut egui::Ui, th: &Theme, g: MockGlyph, name: &str, query: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let sz = th.icon_glyph_size_md.value();
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(sz), egui::Sense::hover());
        g.image(
            sz,
            tasty_ui_widgets::table_cell_ink(ui, th, th.text_secondary()),
        )
        .paint_at(ui, rect);
        ui.label(explorer_match_job(
            th,
            name,
            query,
            egui::FontId::proportional(th.font_size_body.value()),
            tasty_ui_widgets::table_cell_ink(ui, th, th.table_row_fg()),
            tasty_ui_widgets::in_selected_table_row(ui),
        ));
    });
}

const HITS: &[(&str, &str, &str, &str)] = &[
    ("report.pdf", ".", "2.4 MB", "2026-06-24 09:12"),
    (
        "report-final.pdf",
        "Documents",
        "2.6 MB",
        "2026-10-02 10:14",
    ),
    (
        "q3-report.xlsx",
        "Documents/finance",
        "88 KB",
        "2026-09-30 17:02",
    ),
    (
        "report.md",
        "mockup-exports/notes",
        "4 KB",
        "2026-07-01 08:40",
    ),
];

/// 하위 폴더 검색 결과 표 — Name · Folder · Size · Date.
fn hit_table(ui: &mut egui::Ui, theme: &Theme, rows: &[(&str, &str, &str, &str)], id: &str) {
    let columns = explorer_detail_columns(theme, heads(Some(t("explorer.type.folder"))));
    let sel = rows.len().checked_sub(2);
    Table::new(columns)
        .active_sort(0_usize, TableSortDir::Asc)
        .header_fill(theme.table_header_bg().to_egui())
        .header_pad_right(theme.spacing_sm)
        .id_salt(id)
        .show(
            ui,
            theme,
            rows,
            |r| sel.is_some_and(|s| rows[s].0 == r.0),
            |ui, th, r, col| {
                let muted = tasty_ui_widgets::table_cell_ink(ui, th, th.text_muted());
                let mono = egui::FontId::monospace(th.font_size_caption.value());
                match col {
                    0 => name_cell(ui, th, icons::FILE, r.0, "report"),
                    1 => {
                        ui.add(
                            egui::Label::new(egui::RichText::new(r.1).font(mono).color(muted))
                                .truncate(),
                        );
                    }
                    2 => {
                        ui.add_space(th.spacing_sm.value());
                        ui.label(egui::RichText::new(r.2).font(mono).color(muted));
                    }
                    _ => {
                        ui.label(egui::RichText::new(r.3).font(mono).color(muted));
                    }
                }
            },
        );
}

pub fn draw_subfolder_search(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        let block = |ui: &mut egui::Ui, label: &str, add: &dyn Fn(&mut egui::Ui)| {
            wrap_item(ui, |ui| {
                ui.set_width(SEARCH_W.value());
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                lbl(ui, theme, label);
                add(ui);
            });
        };
        block(ui, "searching — partial results", &|ui| {
            find_cell(
                ui,
                theme,
                "report",
                Some(true),
                ExplorerFindStatus::Searching {
                    text: &t_fmt("explorer.find.searching", "4"),
                    stop: t("explorer.find.stop"),
                },
                "Documents/finance/q3-report.xlsx",
                |ui| hit_table(ui, theme, HITS, "gallery_exp_hits_running"),
            );
        });
        block(ui, "done — 2 folders skipped", &|ui| {
            find_cell(
                ui,
                theme,
                "report",
                Some(true),
                ExplorerFindStatus::Skipped {
                    text: &t_fmt("explorer.find.found", "4"),
                    skipped: &t_fmt("explorer.find.skipped", "2"),
                    tooltip: "~/Downloads/private\n~/Downloads/.Trash",
                },
                &t_fmt("explorer.status.items", "4"),
                |ui| hit_table(ui, theme, &HITS[..1], "gallery_exp_hits_skipped"),
            );
        });
        block(ui, "stopped", &|ui| {
            find_cell(
                ui,
                theme,
                "report",
                Some(true),
                ExplorerFindStatus::Text(&t_fmt("explorer.find.stopped", "4")),
                &t_fmt("explorer.status.items", "4"),
                |ui| hit_table(ui, theme, &HITS[..1], "gallery_exp_hits_stopped"),
            );
        });
        block(ui, "no results", &|ui| {
            find_cell(
                ui,
                theme,
                "invoice-2019",
                Some(true),
                ExplorerFindStatus::Text(&t_fmt("explorer.find.found", "0")),
                &t_fmt("explorer.status.items", "0"),
                |ui| {
                    let none = t_fmt("explorer.find.none", "Downloads");
                    CenterState::empty(icons::SEARCH, &none)
                        .sub_line(Some(t("explorer.find.none_sub")))
                        .show(ui, theme, None);
                },
            );
        });
        block(ui, "filter — no matches", &|ui| {
            find_cell(
                ui,
                theme,
                "rezzz",
                Some(false),
                ExplorerFindStatus::Text(&t_fmt2("explorer.find.count", "0", "6")),
                &t_fmt2("explorer.find.status", "0", "6"),
                |ui| {
                    let none = t("explorer.find.none_filter").replace("{query}", "rezzz");
                    CenterState::empty(icons::SEARCH, &none).show(ui, theme, None);
                },
            );
        });
        block(ui, "search — cap reached · 1 folder skipped", &|ui| {
            find_cell(
                ui,
                theme,
                ".rs",
                Some(true),
                ExplorerFindStatus::Skipped {
                    text: &t("explorer.find.capped").replace("{n}", "5,000"),
                    skipped: &t_count("explorer.find.skipped", 1, &["1"]),
                    tooltip: "~/Downloads/private",
                },
                &t("explorer.find.capped_hint").replace("{n}", "5,000"),
                |ui| hit_table(ui, theme, &HITS[..1], "gallery_exp_hits_capped"),
            );
        });
        block(ui, "error — the start folder can't be read", &|ui| {
            find_cell(
                ui,
                theme,
                "report",
                Some(true),
                ExplorerFindStatus::Text("—"),
                "",
                |ui| {
                    CenterState::error(t("explorer.find.failed"))
                        .sub_line(Some("Permission denied (os error 13)"))
                        .action(t("explorer.state.read_error_retry"), None)
                        .show(ui, theme, None);
                },
            );
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "switch",
                "Subfolders checkbox in the bar (hidden on remote)",
            ),
            (
                "running",
                "Spinner 14 · “Searching… {n} found” · Stop (ghost sm) · results stream in",
            ),
            (
                "Detail",
                "Name · Folder (explorer-search-folder-col-width 160, mono, relative) · Size · Date",
            ),
            (
                "List / Grid",
                "unchanged rows · folder in tooltip + status line",
            ),
            (
                "skipped",
                "“{n} folders skipped” · accent-warning · tooltip lists them",
            ),
            (
                "no results",
                "state screen · search glyph · “No matches in {folder}”",
            ),
            (
                "filter 0",
                "bar “0 of {total}” · search glyph · “No names match “{query}”” · no sub-line",
            ),
            ("skipped one", "“1 folder skipped”"),
            (
                "cap",
                "5,000 results · “{n}+ found · stopped” · status line hint · results kept",
            ),
            ("error", "state screen · error tone · Retry"),
            ("stopped", "“Stopped · {n} found” · results kept"),
            ("leave", "untick → filter · Esc / × / navigation → folder"),
        ],
        &[
            TokenChip::without_color("explorer-search-folder-col-width", "→ size-160"),
            TokenChip::new(
                "spinner-indicator",
                "searching",
                theme.spinner_indicator().to_egui(),
            ),
            TokenChip::new(
                "accent-warning",
                "skipped count",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "explorer-error-fg",
                "search failed",
                theme.explorer_error_fg().to_egui(),
            ),
        ],
    );
    note(
        ui,
        theme,
        "i18n: explorer.find.filter_placeholder · search_placeholder · subfolders · count · \
         searching · stop · stopped · skipped · skipped_one · capped · capped_hint · none · \
         none_filter · failed.",
    );
}
