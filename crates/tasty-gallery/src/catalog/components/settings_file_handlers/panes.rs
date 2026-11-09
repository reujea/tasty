//! Pane 그리기 — 머리 · 추가 카드 · 목록. 본체와 같은 `fh_*` 부품을 쓴다.

use super::*;

pub(super) fn det_row_view<'a>(
    l: &'a FhCopy,
    x: &'a Det,
    sources: &'a str,
    locked: &'a str,
) -> FhRowView<'a> {
    let others = x.others();
    let ur = x.user_rules();
    let un = x.pending == Some(DetPending::Unpatch);
    let mut origins: Vec<FhOrigin> = others
        .iter()
        .map(|s| FhOrigin {
            label: s.clone(),
            plugin: s.starts_with("plugin:"),
            struck: false,
        })
        .collect();
    if x.has_user() {
        origins.push(FhOrigin {
            label: if ur {
                "user".into()
            } else {
                l.user_override.into()
            },
            plugin: false,
            struck: un,
        });
    }
    let pending = match x.pending {
        Some(DetPending::Add) => Some(l.added),
        Some(DetPending::Remove) => Some(l.removed),
        Some(DetPending::Patch) => Some(if x.prev.as_ref().is_some_and(|p| p.has_user()) {
            l.yours_replaced
        } else {
            l.yours_added
        }),
        Some(DetPending::Unpatch) => Some(if ur {
            l.yours_removed
        } else {
            l.override_removed
        }),
        None => None,
    };
    let summary = x
        .rules
        .iter()
        .flat_map(|r| {
            r.parts.iter().map(move |p| FhSummarySeg {
                text: p.clone(),
                struck: un && r.src == "user",
            })
        })
        .collect();
    let rm = x.pending == Some(DetPending::Remove);
    FhRowView {
        id: &x.id,
        origins,
        prio: None,
        pending,
        strike_id: rm,
        on: x.on && !rm,
        switch_on: if un { x.base_on.unwrap_or(x.on) } else { x.on },
        switch_disabled: rm || un,
        trailing: if !x.has_user() {
            FhTrailing::Lock
        } else if x.pending.is_some() {
            FhTrailing::Undo
        } else {
            FhTrailing::Remove
        },
        locked_tooltip: locked,
        summary,
        undo_label: l.undo,
        remove_label: l.remove,
        sources_label: sources,
    }
}

pub(super) fn detectors_pane(ui: &mut egui::Ui, th: &Theme, l: &FhCopy, st: &mut DetState) {
    ui.spacing_mut().item_spacing.y = th.spacing_md.value();
    if fh_intro(ui, th, l.det_intro, &l.bullets, l.add_det, st.adding) {
        st.adding = true;
    }
    if st.adding {
        det_card(ui, th, l, st);
    }
    fh_list_head(ui, th, &fill(l.reg_det, &st.xs.len().to_string()));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if st.xs.is_empty() {
            fh_empty(ui, th, l.empty_det);
        }
        let mut act: Option<(usize, tasty_ui_widgets::FhRowEvents)> = None;
        for (i, x) in st.xs.iter().enumerate() {
            let sources = fill(
                l.sources,
                &(x.others().len() + usize::from(x.has_user())).to_string(),
            );
            let locked = fill(l.locked, &x.others().join(", "));
            let view = det_row_view(l, x, &sources, &locked);
            let ev = ui.push_id(("det", i), |ui| fh_row(ui, th, &view)).inner;
            if ev != Default::default() {
                act = Some((i, ev));
            }
        }
        if let Some((i, ev)) = act {
            det_event(st, i, ev);
        }
    });
}

pub(super) fn det_event(st: &mut DetState, i: usize, ev: tasty_ui_widgets::FhRowEvents) {
    let Some(x) = st.xs.get_mut(i) else { return };
    if let Some(on) = ev.toggled {
        x.on = on;
    }
    if ev.remove {
        x.pending = Some(if x.others().is_empty() {
            DetPending::Remove
        } else {
            DetPending::Unpatch
        });
    }
    if ev.undo {
        match x.pending {
            Some(DetPending::Add) => {
                st.xs.remove(i);
            }
            Some(DetPending::Patch) => {
                if let Some(prev) = x.prev.take() {
                    *x = *prev;
                }
            }
            _ => *x = x.strip(),
        }
    }
}

pub(super) fn det_card(ui: &mut egui::Ui, th: &Theme, l: &FhCopy, st: &mut DetState) {
    let id = st.id.trim().to_string();
    let exts: Vec<String> = st
        .ext
        .split(',')
        .map(|s| s.trim().trim_start_matches('.').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let exists = st.xs.iter().position(|x| x.id == id && !id.is_empty());
    let err_id = if id.is_empty() {
        None
    } else if let Some(rest) = id.strip_prefix('$') {
        if !valid_chars(rest, 64) {
            Some(l.err_det_chars)
        } else if exists.is_none() {
            Some(l.err_reserved)
        } else {
            None
        }
    } else if !valid_chars(&id, 64) {
        Some(l.err_det_chars)
    } else {
        None
    };
    let info = match exists {
        Some(i) if err_id.is_none() => {
            let x = &st.xs[i];
            let base = if x.pending == Some(DetPending::Patch) {
                x.prev.as_deref().unwrap_or(x)
            } else {
                x
            };
            Some(if base.has_user() || x.pending == Some(DetPending::Add) {
                fill(l.info_replace_det, &id)
            } else {
                fill(l.info_patch, &id)
            })
        }
        _ => None,
    };
    let err_rules = (exts.is_empty() && st.glob.trim().is_empty()).then_some(l.err_rules);
    let tried = st.tried;
    let labels = FhCardLabels {
        title: l.new_det,
        cancel: l.cancel,
        add: if exists.is_some() && err_id.is_none() {
            l.upd_det
        } else {
            l.add_det
        },
    };
    let ev = fh_card(
        ui,
        th,
        labels,
        if tried { err_rules } else { None },
        !id.is_empty(),
        |ui| {
            fh_field(
                ui,
                th,
                l.id,
                FhLine::pick(
                    if tried { err_id } else { None },
                    info.as_deref(),
                    Some(l.det_id_hint),
                ),
                |ui| {
                    Input::new()
                        .ground(Ground::Raised)
                        .mono(true)
                        .placeholder("my-detector")
                        .invalid(tried && err_id.is_some())
                        .show(ui, th, &mut st.id);
                },
            );
            fh_field(ui, th, l.ext, FhLine::Caption(l.ext_hint), |ui| {
                Input::new()
                    .ground(Ground::Raised)
                    .mono(true)
                    .placeholder("md, mdx")
                    .invalid(tried && err_rules.is_some())
                    .show(ui, th, &mut st.ext);
            });
            fh_field(ui, th, l.glob, FhLine::None, |ui| {
                Input::new()
                    .ground(Ground::Raised)
                    .mono(true)
                    .placeholder("Dockerfile")
                    .invalid(tried && err_rules.is_some())
                    .show(ui, th, &mut st.glob);
            });
        },
    );
    if ev.cancel {
        close_det(st);
    } else if ev.add {
        st.tried = true;
        if err_id.is_some() || err_rules.is_some() {
            return;
        }
        let mut parts = Vec::new();
        if !exts.is_empty() {
            parts.push(format!("ext {}", exts.join(", ")));
        }
        if !st.glob.trim().is_empty() {
            parts.push(format!("path {}", st.glob.trim()));
        }
        let rule = Rule {
            src: "user".into(),
            parts,
        };
        match exists {
            None => st.xs.push(Det {
                pending: Some(DetPending::Add),
                rules: vec![rule],
                ..Det::new(&id, &[], true)
            }),
            Some(i) => {
                let x = &mut st.xs[i];
                if x.pending == Some(DetPending::Add) {
                    x.rules = vec![rule];
                } else {
                    let prev = if x.pending.is_some() {
                        x.prev.clone().map(|p| *p).unwrap_or_else(|| x.strip())
                    } else {
                        x.clone()
                    };
                    let mut next = x.strip();
                    next.rules.retain(|r| r.src != "user");
                    next.rules.push(rule);
                    next.user_meta = false;
                    next.pending = Some(DetPending::Patch);
                    next.prev = Some(Box::new(prev));
                    *x = next;
                }
            }
        }
        close_det(st);
    }
}

pub(super) fn close_det(st: &mut DetState) {
    st.adding = false;
    st.tried = false;
    st.id.clear();
    st.ext.clear();
    st.glob.clear();
}

pub(super) fn handlers_pane(ui: &mut egui::Ui, th: &Theme, l: &FhCopy, st: &mut HdlState) {
    ui.spacing_mut().item_spacing.y = th.spacing_md.value();
    if fh_intro(ui, th, l.hdl_intro, &l.bullets, l.add_hdl, st.adding) {
        st.adding = true;
    }
    if st.adding {
        hdl_card(ui, th, l, st);
    }
    let order = run_order(&st.xs);
    fh_list_head(ui, th, &fill(l.reg_hdl, &st.xs.len().to_string()));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if st.xs.is_empty() {
            fh_empty(ui, th, l.empty_hdl);
        }
        let mut act = None;
        for &i in &order {
            let x = &st.xs[i];
            let rm = x.pending == Some(HdlPending::Remove);
            let locked = fill(l.locked, &x.origin);
            let prio = crate::i18n::t_fmt(
                "settings.file_handler.hook_handlers.prio",
                &x.prio.to_string(),
            );
            let view = FhRowView {
                id: &x.id,
                origins: vec![FhOrigin {
                    label: x.origin.clone(),
                    plugin: x.origin.starts_with("plugin:"),
                    struck: false,
                }],
                prio: Some(&prio),
                pending: x.pending.map(|p| match p {
                    HdlPending::Add => l.added,
                    HdlPending::Remove => l.removed,
                    HdlPending::Replace => l.replaced,
                }),
                strike_id: rm,
                on: x.on && !rm,
                switch_on: x.on,
                switch_disabled: rm,
                trailing: if x.origin != "user" {
                    FhTrailing::Lock
                } else if x.pending.is_some() {
                    FhTrailing::Undo
                } else {
                    FhTrailing::Remove
                },
                locked_tooltip: &locked,
                summary: vec![FhSummarySeg {
                    text: format!("{}  →  {}", x.det, x.act),
                    struck: false,
                }],
                undo_label: l.undo,
                remove_label: l.remove,
                sources_label: "",
            };
            let ev = ui.push_id(("hdl", i), |ui| fh_row(ui, th, &view)).inner;
            if ev != Default::default() {
                act = Some((i, ev));
            }
        }
        if let Some((i, ev)) = act {
            hdl_event(st, i, ev);
        }
    });
}

pub(super) fn hdl_event(st: &mut HdlState, i: usize, ev: tasty_ui_widgets::FhRowEvents) {
    let Some(x) = st.xs.get_mut(i) else { return };
    if let Some(on) = ev.toggled {
        x.on = on;
    }
    if ev.remove {
        x.pending = Some(HdlPending::Remove);
    }
    if ev.undo {
        match x.pending {
            Some(HdlPending::Add) => {
                st.xs.remove(i);
            }
            Some(HdlPending::Replace) => {
                if let Some(prev) = x.prev.take() {
                    *x = *prev;
                }
            }
            _ => {
                x.pending = None;
                x.prev = None;
            }
        }
    }
}

pub(super) fn hdl_card(ui: &mut egui::Ui, th: &Theme, l: &FhCopy, st: &mut HdlState) {
    let dets: Vec<String> = det_seed().into_iter().map(|d| d.id).collect();
    let short = st.short.trim().to_string();
    let full = format!("user/{short}");
    let exists = st.xs.iter().position(|x| x.id == full && !short.is_empty());
    let err_short = (!short.is_empty() && !valid_chars(&short, 32)).then_some(l.err_hdl_chars);
    let prio = st.prio.trim().to_string();
    let err_prio = (!prio.is_empty() && prio.parse::<i32>().is_err()).then_some(l.err_int);
    let method = st.method.trim().to_string();
    let err_method =
        (st.kind == 1 && !method.is_empty() && !dotted(&method)).then_some(l.err_method);
    let req = match st.kind {
        0 => !st.surface.trim().is_empty(),
        1 => !method.is_empty(),
        _ => true,
    };
    let can_add = !short.is_empty() && st.det.is_some() && req;
    let tried = st.tried;
    let shown = |e: Option<&'static str>| if tried { e } else { None };
    let saved = fill(l.saved_as, if short.is_empty() { "user/…" } else { &full });
    let info = (exists.is_some() && err_short.is_none()).then(|| fill(l.info_replace_hdl, &full));
    let labels = FhCardLabels {
        title: l.new_hdl,
        cancel: l.cancel,
        add: if exists.is_some() && err_short.is_none() {
            l.upd_hdl
        } else {
            l.add_hdl
        },
    };
    let ev = fh_card(ui, th, labels, None, can_add, |ui| {
        fh_field(
            ui,
            th,
            l.short,
            FhLine::pick(shown(err_short), info.as_deref(), Some(&saved)),
            |ui| {
                Input::new()
                    .ground(Ground::Raised)
                    .mono(true)
                    .placeholder("my-viewer")
                    .invalid(shown(err_short).is_some())
                    .show(ui, th, &mut st.short);
            },
        );
        fh_field(ui, th, l.detector, FhLine::None, |ui| {
            let names: Vec<&str> = dets.iter().map(String::as_str).collect();
            let w = th
                .file_handler_select_width()
                .min(LogicalPx(ui.available_width()))
                .value();
            tasty_ui_widgets::select_or_placeholder_on(
                ui,
                th,
                Ground::Raised,
                "gallery_fh_detector",
                &mut st.det,
                &names,
                l.pick_det,
                w,
                true,
            );
        });
        fh_field(
            ui,
            th,
            l.priority,
            FhLine::pick(shown(err_prio), None, Some(l.prio_hint)),
            |ui| {
                Input::new()
                    .ground(Ground::Raised)
                    .mono(true)
                    .placeholder("100")
                    .invalid(shown(err_prio).is_some())
                    .width(th.file_handler_priority_width().value())
                    .show(ui, th, &mut st.prio);
            },
        );
        let caption = if st.kind == 2 {
            FhLine::Caption(l.system_hint)
        } else {
            FhLine::None
        };
        fh_field(ui, th, l.action, caption, |ui| {
            let w = th
                .file_handler_kind_width()
                .min(LogicalPx(ui.available_width()))
                .value();
            tasty_ui_widgets::select_on(
                ui,
                th,
                Ground::Raised,
                "gallery_fh_kind",
                &mut st.kind,
                &l.kinds,
                w,
                true,
            );
        });
        match st.kind {
            0 => {
                fh_field(ui, th, l.surface, FhLine::None, |ui| {
                    Input::new()
                        .ground(Ground::Raised)
                        .mono(true)
                        .placeholder("markdown_view")
                        .show(ui, th, &mut st.surface);
                });
                fh_field(ui, th, l.param, FhLine::Caption(l.param_hint), |ui| {
                    Input::new()
                        .ground(Ground::Raised)
                        .mono(true)
                        .placeholder("file")
                        .show(ui, th, &mut st.param);
                });
            }
            1 => {
                fh_field(
                    ui,
                    th,
                    l.method,
                    FhLine::pick(shown(err_method), None, None),
                    |ui| {
                        Input::new()
                            .ground(Ground::Raised)
                            .mono(true)
                            .placeholder("com.example.foo.open")
                            .invalid(shown(err_method).is_some())
                            .show(ui, th, &mut st.method);
                    },
                );
            }
            _ => {}
        }
    });
    if ev.cancel {
        close_hdl(st);
    } else if ev.add {
        st.tried = true;
        if err_short.is_some() || err_prio.is_some() || err_method.is_some() {
            return;
        }
        let act = match st.kind {
            0 => format!("surface:{}", st.surface.trim()),
            1 => format!("ipc:{method}"),
            _ => "system".into(),
        };
        let det = st
            .det
            .and_then(|i| dets.get(i).cloned())
            .unwrap_or_default();
        let next = Hdl::new(&full, prio.parse().unwrap_or(100), "user", &det, &act, true);
        match exists {
            None => st.xs.push(Hdl {
                pending: Some(HdlPending::Add),
                ..next
            }),
            Some(i) => {
                let x = &mut st.xs[i];
                *x = if x.pending == Some(HdlPending::Add) {
                    Hdl {
                        pending: Some(HdlPending::Add),
                        ..next
                    }
                } else {
                    let prev = match (x.pending, x.prev.take()) {
                        (Some(_), Some(p)) => *p,
                        _ => Hdl {
                            pending: None,
                            prev: None,
                            ..x.clone()
                        },
                    };
                    Hdl {
                        pending: Some(HdlPending::Replace),
                        prev: Some(Box::new(prev)),
                        ..next
                    }
                };
            }
        }
        close_hdl(st);
    }
}

pub(super) fn close_hdl(st: &mut HdlState) {
    *st = HdlState {
        xs: std::mem::take(&mut st.xs),
        ..HdlState::default()
    };
}
