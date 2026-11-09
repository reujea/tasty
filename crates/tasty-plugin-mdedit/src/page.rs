//! 편집기 문서 조립. 미리보기 렌더 결과([`crate::preview::render`])에 편집기 요소(툴바·배너·
//! 패널·원문 편집기)와 스크립트를 덧붙인다. 부분 갱신용 조각([`chrome_html`])과 그것을 웹뷰에
//! 밀어 넣는 메시지([`apply_msg`])도 여기서 만든다.

use serde_json::{Value, json};
use tasty_plugin_sdk::Translator;
use tasty_type_appearance::theme::Theme;

use crate::core::blocks;
use crate::core::merge::Choice;
use crate::core::review::{self, Origin};
use crate::core::snapshot::SnapshotInfo;
use crate::core::textio;
use crate::core::units::{self, Entry};
use crate::session::{Level, Mode, Panel, Session};

const EDITOR_JS: &str = include_str!("../assets/editor.js");
const EDITOR_CSS: &str = include_str!("../assets/editor.css");

/// 웹뷰 → plugin 메시지 한 조각의 상한(인코딩 뒤 글자 수, docs/surfaces/mdedit/index.md#웹뷰-입력은-nav-fragment-로-보낸다).
pub const CHUNK_LIMIT: usize = 16 * 1024;

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

/// `<script>` 안에 실을 JSON — `</script>` 로 문서가 끊기지 않게 `<` 를 이스케이프한다.
fn script_json(v: &Value) -> String {
    v.to_string().replace('<', "\\u003c")
}

/// 배너·diff 배경 — 상태색을 낮은 알파로 깐다(render.rs 의 콜아웃 배경과 같은 도출).
const STATE_BG_ALPHA: u8 = 46;
/// 정보 배너 배경.
const TINT_ALPHA: u8 = 38;
/// 에이전트 기록 블록 배경 — 본문 위에 깔리므로 더 옅다.
const AGENT_NOTE_ALPHA: u8 = 20;

fn theme_vars(theme: &Theme) -> String {
    format!(
        ":root{{--mde-bar-bg:{bar};--mde-sep:{sep};--mde-muted:{muted};--mde-accent:{accent};\
         --mde-on-accent:{on_accent};--mde-warn:{warn};--mde-danger:{danger};\
         --mde-info-bg:{info_bg};--mde-warn-bg:{warn_bg};--mde-danger-bg:{danger_bg};\
         --mde-success-bg:{success_bg};--mde-agent-bg:{agent_bg};}}",
        bar = theme.bg_sidebar().to_hex(),
        sep = theme.separator.unpremultiplied().to_hex(),
        muted = theme.text_muted().to_hex(),
        accent = theme.accent_primary().to_hex(),
        on_accent = theme.text_on_accent().to_hex(),
        warn = theme.accent_warning().to_hex(),
        danger = theme.accent_danger().to_hex(),
        info_bg = theme.accent_primary().with_alpha(TINT_ALPHA).to_hex(),
        warn_bg = theme.accent_warning().with_alpha(STATE_BG_ALPHA).to_hex(),
        danger_bg = theme.accent_danger().with_alpha(STATE_BG_ALPHA).to_hex(),
        success_bg = theme.accent_success().with_alpha(STATE_BG_ALPHA).to_hex(),
        agent_bg = theme.accent_primary().with_alpha(AGENT_NOTE_ALPHA).to_hex(),
    )
}

fn status(s: &Session, tr: &Translator) -> (String, &'static str) {
    if s.conflict.is_some() {
        (tr.t("mdedit.status.conflict").to_string(), "conflict")
    } else if s.load_error.is_some() {
        (tr.t("mdedit.status.load_failed").to_string(), "conflict")
    } else if s.dirty() {
        (tr.t("mdedit.status.dirty").to_string(), "dirty")
    } else {
        (tr.t("mdedit.status.saved").to_string(), "")
    }
}

fn level_class(l: Level) -> &'static str {
    match l {
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    }
}

fn origin_tag(o: Origin, tr: &Translator) -> String {
    let (class, key) = match o {
        Origin::Agent => ("agent", "mdedit.origin.agent"),
        Origin::External => ("external", "mdedit.origin.external"),
    };
    format!(r#"<span class="mde-tag {class}">{}</span>"#, esc(tr.t(key)))
}

/// 툴바 + 배너 + 패널. `#mdedit-chrome` 의 innerHTML 이 된다.
pub fn chrome_html(s: &Session, tr: &Translator, snapshots: &[SnapshotInfo]) -> String {
    let (status_text, status_class) = status(s, tr);
    let on = |m: Mode| if s.mode == m { " class=\"on\"" } else { "" };
    let hunks = s.review_hunks();
    let review_btn = if hunks.is_empty() {
        String::new()
    } else {
        format!(
            r#"<button data-act="panel" data-arg="review">{}<span class="mde-count">{}</span></button>"#,
            esc(tr.t("mdedit.toolbar.review")),
            hunks.len()
        )
    };
    let save_disabled = if s.dirty() && s.conflict.is_none() {
        ""
    } else {
        " disabled"
    };
    let mut h = format!(
        r#"<div class="mde-bar"><div class="mde-seg"><button data-act="mode" data-arg="preview"{p}>{pl}</button><button data-act="mode" data-arg="source"{sr}>{sl}</button></div><button data-act="save" class="mde-primary"{sd}>{save}</button><span class="mde-status {sc}" title="{hash}">{st}</span><span class="mde-blockfmt" id="mde-blockfmt"></span><span class="mde-file" title="{path}">{name}</span><span class="mde-spacer"></span>{review_btn}<button data-act="panel" data-arg="snapshots">{snap}</button></div>"#,
        p = on(Mode::Preview),
        pl = esc(tr.t("mdedit.toolbar.preview")),
        sr = on(Mode::Source),
        sl = esc(tr.t("mdedit.toolbar.source")),
        sd = save_disabled,
        save = esc(tr.t("mdedit.toolbar.save")),
        sc = status_class,
        hash = esc(&s.disk_hash().unwrap_or_default()),
        st = esc(&status_text),
        path = esc(&s.path.to_string_lossy()),
        name = esc(&s.file_name()),
        snap = esc(tr.t("mdedit.toolbar.snapshots")),
    );
    if let Some(b) = &s.banner {
        h.push_str(&format!(
            r#"<div class="mde-banner {}"><span>{}</span><button data-act="dismiss" title="{}">&#215;</button></div>"#,
            level_class(b.level),
            esc(&tr.t_fmt(b.key, &b.arg)),
            esc(tr.t("mdedit.toolbar.dismiss")),
        ));
    }
    if s.conflict.is_some() {
        h.push_str(&conflict_panel(s, tr));
    } else if s.panel == Panel::Review && !hunks.is_empty() {
        h.push_str(&review_panel(&hunks, tr));
    } else if s.panel == Panel::Snapshots {
        h.push_str(&snapshot_panel(snapshots, tr));
    }
    h
}

fn conflict_panel(s: &Session, tr: &Translator) -> String {
    let Some(c) = &s.conflict else {
        return String::new();
    };
    let mut h = format!(
        r#"<div class="mde-panel"><div class="mde-panel-head">{}<span class="mde-spacer"></span><button data-act="resolve" class="mde-primary"{}>{}</button></div>"#,
        esc(tr.t("mdedit.conflict.title")),
        if c.choices.iter().all(Option::is_some) {
            ""
        } else {
            " disabled"
        },
        esc(tr.t("mdedit.conflict.apply")),
    );
    for (i, x) in c.merge.conflicts().enumerate() {
        let chosen = c.choices.get(i).copied().flatten();
        let mark = |want: Choice| {
            if chosen == Some(want) {
                " mde-chosen"
            } else {
                ""
            }
        };
        h.push_str(&format!(
            r#"<div class="mde-item"><div class="mde-item-head">{line}<span class="mde-spacer"></span><button data-act="choose" data-arg="{i}:ours" class="{mo}">{ours}</button><button data-act="choose" data-arg="{i}:theirs" class="{mt}">{theirs}</button><button data-act="choose" data-arg="{i}:both" class="{mb}">{both}</button></div><pre class="mde-ours">{o}</pre><pre class="mde-theirs">{t}</pre></div>"#,
            line = esc(&tr.t_fmt("mdedit.conflict.at_line", &x.base_line.to_string())),
            mo = mark(Choice::Ours),
            mt = mark(Choice::Theirs),
            mb = mark(Choice::Both),
            ours = esc(tr.t("mdedit.conflict.ours")),
            theirs = esc(tr.t("mdedit.conflict.theirs")),
            both = esc(tr.t("mdedit.conflict.both")),
            o = esc(&x.ours),
            t = esc(&x.theirs),
        ));
    }
    h.push_str("</div>");
    h
}

fn review_panel(hunks: &[review::ReviewHunk], tr: &Translator) -> String {
    let mut h = format!(
        r#"<div class="mde-panel"><div class="mde-panel-head">{}<span class="mde-spacer"></span><button data-act="keep_all">{}</button></div>"#,
        esc(tr.t("mdedit.review.title")),
        esc(tr.t("mdedit.review.keep_all")),
    );
    for hk in hunks {
        h.push_str(&format!(
            r#"<div class="mde-item"><div class="mde-item-head">{tag}{line}<span class="mde-spacer"></span><button data-act="keep" data-arg="{i}">{keep}</button><button data-act="revert" data-arg="{i}">{revert}</button></div>{del}{add}</div>"#,
            tag = origin_tag(hk.origin, tr),
            line = esc(&tr.t_fmt("mdedit.review.at_line", &hk.line.to_string())),
            i = hk.index,
            keep = esc(tr.t("mdedit.review.keep")),
            revert = esc(tr.t("mdedit.review.revert")),
            del = if hk.removed.is_empty() {
                String::new()
            } else {
                format!(r#"<pre class="mde-del">{}</pre>"#, esc(&hk.removed))
            },
            add = if hk.added.is_empty() {
                String::new()
            } else {
                format!(r#"<pre class="mde-add">{}</pre>"#, esc(&hk.added))
            },
        ));
    }
    h.push_str("</div>");
    h
}

fn snapshot_panel(snaps: &[SnapshotInfo], tr: &Translator) -> String {
    let mut h = format!(
        r#"<div class="mde-panel"><div class="mde-panel-head">{}</div>"#,
        esc(tr.t("mdedit.snapshots.title"))
    );
    if snaps.is_empty() {
        h.push_str(&esc(tr.t("mdedit.snapshots.empty")));
    }
    for s in snaps {
        let when = chrono::DateTime::from_timestamp_millis(s.at_ms as i64)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let reason_key = format!("mdedit.snapshots.reason.{}", s.reason);
        h.push_str(&format!(
            r#"<div class="mde-item"><div class="mde-item-head">{when} · {reason} · {bytes} B<span class="mde-spacer"></span><button data-act="restore" data-arg="{id}">{restore}</button></div></div>"#,
            when = esc(&when),
            reason = esc(tr.t(&reason_key)),
            bytes = s.bytes,
            id = esc(&s.id),
            restore = esc(tr.t("mdedit.snapshots.restore")),
        ));
    }
    h.push_str("</div>");
    h
}

fn gutter(s: &Session) -> Value {
    let lines = s
        .review
        .as_ref()
        .map(|r| review::changed_lines(&r.base, &r.target, &s.agent))
        .unwrap_or_default();
    Value::Array(
        lines
            .into_iter()
            .map(|(l, o)| {
                json!([
                    l,
                    match o {
                        Origin::Agent => "agent",
                        Origin::External => "external",
                    }
                ])
            })
            .collect(),
    )
}

/// 미리보기 편집 단위(docs/surfaces/mdedit/index.md#미리보기-편집-단위는-블록-원문-조각) — `[종류, 원문 조각, 렌더 글자, 플래그]`. 플래그 비트:
/// 1 열 수 있음 · 2 여러 줄 · 4 뒤에 새 블록 · 8 체크 항목 · 16 머리 행 셀 · 32 지울 수 있는 빈 항목.
/// 번호는 배열 위치다. 미리보기가 아니거나 읽기 전용이면 `null` — 편집도 줄 추가도 안 연다
/// (빈 배열은 빈 문서다).
fn preview_blocks(s: &Session) -> Value {
    if s.mode != Mode::Preview || s.conflict.is_some() || s.load_error.is_some() {
        return Value::Null;
    }
    let src = s.buffer.as_str();
    Value::Array(
        blocks::blocks(src)
            .into_iter()
            .map(|b| {
                let mut f = 0u32;
                if b.editable {
                    f |= 1;
                }
                if b.multiline {
                    f |= 2;
                }
                if b.after.is_some() {
                    f |= 4;
                }
                if b.item.as_ref().is_some_and(|i| i.task) {
                    f |= 8;
                }
                if b.cell.as_ref().is_some_and(|c| c.head) {
                    f |= 16;
                }
                if b.item.as_ref().is_some_and(|i| i.simple) && b.range.is_empty() {
                    f |= 32;
                }
                json!([b.kind.code(), &src[b.range.clone()], b.text, f])
            })
            .collect(),
    )
}

/// 다시 그린 뒤 열 편집기 — 세션이 남긴 자리(바이트)와 같은 줄에서 그 뒤에 시작하는 첫 편집
/// 단위의 번호. 그 줄에 단위가 없으면(코드 블록을 넣었을 때 등) 아무것도 안 연다.
fn focus_block(s: &Session) -> Value {
    let Some(f) = s.focus else {
        return Value::Null;
    };
    let src = s.buffer.as_str();
    let Some(line) = src
        .get(f.at..)
        .map(|r| r.find('\n').map_or(src.len(), |i| f.at + i))
    else {
        return Value::Null;
    };
    blocks::blocks(src)
        .iter()
        .position(|b| b.editable && (f.at..=line).contains(&b.range.start))
        .map_or(
            Value::Null,
            |i| json!({ "id": i, "after": f.after, "start": f.start }),
        )
}

/// 손잡이 단위(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다) — `{u: [[종류, 목록, 체크, 원문 해시, 편집 단위 번호, 여러 줄]], l: [[번호 목록, 부모
/// 단위]], t: [[0, 단위] | [1, 목록]]}`. 문단 · 제목 · 항목은 이미 글자로 짝지은 편집 단위의 요소를
/// 그대로 쓰도록 그 번호를 싣는다(없으면 -1). 미리보기 모드에서 편집을 받을 때만 싣는다.
fn preview_units(s: &Session) -> Value {
    if s.mode != Mode::Preview || s.conflict.is_some() || s.load_error.is_some() {
        return Value::Null;
    }
    let src = s.buffer.as_str();
    let us = units::units(src);
    let bs = blocks::blocks(src);
    let block_of = |u: &units::Unit| -> i64 {
        bs.iter()
            .position(|b| match u.kind {
                units::UnitKind::Item => b
                    .item
                    .as_ref()
                    .is_some_and(|i| i.line_start == u.range.start),
                units::UnitKind::Para | units::UnitKind::Heading(_) => {
                    b.kind != blocks::Kind::Item
                        && b.kind != blocks::Kind::Cell
                        && u.range.contains(&b.range.start)
                }
                _ => false,
            })
            .map_or(-1, |i| i as i64)
    };
    let opt = |o: Option<usize>| o.map_or(-1, |v| v as i64);
    json!({
        "u": us.units.iter().map(|u| json!([
            u.kind.code(),
            opt(u.list),
            u.task as u8,
            textio::content_hash(&src[u.range.clone()]),
            block_of(u),
            src[u.range.clone()].contains('\n') as u8,
        ])).collect::<Vec<_>>(),
        "l": us.lists.iter().map(|l| json!([l.ordered as u8, opt(l.parent)])).collect::<Vec<_>>(),
        "t": us.top.iter().map(|e| match e {
            Entry::Unit(u) => json!([0, u]),
            Entry::List(k) => json!([1, k]),
        }).collect::<Vec<_>>(),
    })
}

/// 편집기 스크립트가 그리는 문구(블록 삽입 메뉴 · 행 단추 · 안내).
fn editor_strings(tr: &Translator) -> Value {
    let mut m = serde_json::Map::new();
    for k in INLINE_KEYS {
        m.insert(
            (*k).to_string(),
            Value::String(tr.t(&format!("mdedit.inline.{k}")).to_string()),
        );
    }
    Value::Object(m)
}

/// `mdedit.inline.*` 키 — 세 로케일에 모두 있어야 한다(시험이 본다).
pub const INLINE_KEYS: &[&str] = &[
    "hint",
    "hint_multi",
    "no_match",
    "add_line",
    "row_above",
    "row_below",
    "slash_paragraph",
    "slash_h1",
    "slash_h2",
    "slash_h3",
    "slash_bullet",
    "slash_numbered",
    "slash_task",
    "slash_quote",
    "slash_code",
    "slash_table",
    "slash_divider",
    "handle_add",
    "handle_drag",
    "menu_format",
    "menu_up",
    "menu_down",
    "menu_dup",
    "menu_delete",
    "fmt_h4",
    "fmt_h5",
    "fmt_h6",
];

/// 완성 문서. `preview_html` 은 [`crate::preview::render`] 결과.
pub fn document(
    preview_html: &str,
    s: &Session,
    theme: &Theme,
    tr: &Translator,
    snapshots: &[SnapshotInfo],
) -> String {
    let init = json!({
        "file": s.path.to_string_lossy(),
        "mode": match s.mode { Mode::Preview => "preview", Mode::Source => "source" },
        "text": (s.mode == Mode::Source).then(|| s.buffer.clone()),
        "readonly": s.conflict.is_some() || s.load_error.is_some(),
        "gutter": gutter(s),
        "chunk": CHUNK_LIMIT,
        "blocks": preview_blocks(s),
        "focus": focus_block(s),
        "units": preview_units(s),
        "i18n": editor_strings(tr),
    });
    let head = format!("<style>{}{}</style></head>", theme_vars(theme), EDITOR_CSS);
    let top = format!(
        r#"<body><div id="mdedit-chrome">{}</div><div id="mdedit-editor"><div id="mdedit-gutter"></div><textarea id="mdedit-ta" wrap="off" spellcheck="false" autocomplete="off"></textarea></div>"#,
        chrome_html(s, tr, snapshots)
    );
    let tail = format!(
        "<script>window.__MDEDIT_INIT={};</script><script>{}</script></body>",
        script_json(&init),
        EDITOR_JS.replace("</script", "<\\/script")
    );
    preview_html
        .replacen("</head>", &head, 1)
        .replacen("<body>", &top, 1)
        .replacen("</body>", &tail, 1)
}

/// 편집기 스크립트가 `tasty:message` 로 받아 `apply` 에 넘기는 갱신 메시지.
pub fn apply_msg(update: &Value) -> Value {
    serde_json::json!({ "type": "apply", "u": update })
}

/// 편집기 스크립트가 받아 바로 저장하는 메시지.
pub fn save_msg() -> Value {
    serde_json::json!({ "type": "save" })
}

/// 상태줄·배너·패널·거터를 새로 그리는 갱신. `with_text` 면 원문 편집기 내용도 넣는다.
pub fn chrome_update(
    s: &Session,
    tr: &Translator,
    snapshots: &[SnapshotInfo],
    with_text: bool,
) -> Value {
    let mut v = json!({
        "chrome": chrome_html(s, tr, snapshots),
        "gutter": gutter(s),
        "readonly": s.conflict.is_some() || s.load_error.is_some(),
    });
    if with_text && s.mode == Mode::Source {
        v["text"] = Value::String(s.buffer.clone());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_json_cannot_close_the_script_tag() {
        let v = json!({ "text": "</script><b>" });
        assert!(!script_json(&v).contains("</script"));
    }

    #[test]
    fn document_injects_chrome_editor_and_script_once() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        std::fs::write(&p, "# a\n- [ ] x\n").unwrap();
        let s = Session::open(p);
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let tr = Translator::default();
        let base = "<!doctype html><html><head><style></style></head><body><div id=\"tasty-md-body\"></div></body></html>";
        let doc = document(base, &s, &theme, &tr, &[]);
        assert_eq!(doc.matches("id=\"mdedit-chrome\"").count(), 1);
        assert_eq!(doc.matches("window.__MDEDIT_INIT=").count(), 1);
        assert!(doc.contains("--mde-accent:"));
        assert!(doc.ends_with("</body></html>"));
    }

    /// 미리보기에서만 편집 단위를 싣고, 원문 편집기·conflict 에서는 `null` 이다(docs/surfaces/mdedit/index.md#미리보기-편집-단위는-블록-원문-조각).
    #[test]
    fn preview_carries_edit_units_only_when_editable() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        std::fs::write(&p, "# a\n- [ ] x\n").unwrap();
        let mut s = Session::open(p);
        let v = preview_blocks(&s);
        assert_eq!(v[0][0], "h");
        assert_eq!(v[1][1], "x");
        assert_eq!(v[1][3].as_u64().unwrap() & 9, 9, "editable task item");
        s.set_mode(Mode::Source);
        assert!(preview_blocks(&s).is_null());
    }

    #[test]
    fn escape_covers_html_specials() {
        assert_eq!(
            esc(r#"<a href="x">&'"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }
}
