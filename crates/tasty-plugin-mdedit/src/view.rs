//! 웹뷰 쪽 — 문서를 싣고(`webview.set_url`), 부분 갱신하고(`webview.post_message`), 편집기 스크립트가
//! 보낸 메시지(docs/surfaces/mdedit/index.md#웹뷰-입력은-nav-fragment-로-보낸다)를 세션 동작으로 바꾼다.

use serde_json::{Value, json};
use tasty_plugin_protocol::ThemeWire;
use tasty_type_appearance::theme::Theme;

use crate::MdEditPlugin;
use crate::bridge::{self, Message};
use crate::core::merge::Choice;
use crate::core::units::{Entry, Format};
use crate::page;
use crate::preview::{self, PreviewInput, render::LinkClick};
use crate::session::{Changes, Ctx, Mode, Panel, Then, UnitOp};

impl MdEditPlugin {
    pub(crate) fn post(&self, surface_id: u32, msg: &Value) {
        let Some(host) = &self.host else {
            return;
        };
        if let Err(e) = host.call(
            "webview.post_message",
            json!({ "surface_id": surface_id, "data": msg }),
        ) {
            tracing::warn!("mdedit surface {surface_id}: webview.post_message failed: {e}");
        }
    }

    fn snapshots_for(&self, surface_id: u32) -> Vec<crate::core::snapshot::SnapshotInfo> {
        let Some(s) = self.sessions.get(&surface_id) else {
            return Vec::new();
        };
        if s.panel != Panel::Snapshots {
            return Vec::new();
        }
        self.store.list(&s.path).unwrap_or_else(|e| {
            tracing::warn!("mdedit: snapshot list failed: {e}");
            Vec::new()
        })
    }

    /// 문서를 통째로 다시 싣는다. 원문 편집기의 커서·스크롤과 미리보기 스크롤은 스크립트가
    /// sessionStorage 로 되살린다.
    pub(crate) fn reload_page(&mut self, surface_id: u32) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let theme = match host
            .call("theme.query", json!({}))
            .map_err(|e| e.to_string())
            .and_then(|v| serde_json::from_value::<ThemeWire>(v).map_err(|e| e.to_string()))
        {
            Ok(w) => Theme::with_colors_and_zoom(w.colors, w.is_light, w.ui_zoom),
            Err(e) => {
                tracing::warn!(
                    "mdedit surface {surface_id}: theme.query failed: {e} — skipping load"
                );
                return;
            }
        };
        let snapshots = self.snapshots_for(surface_id);
        let Some(s) = self.sessions.get_mut(&surface_id) else {
            return;
        };
        s.expect_seq = 1;
        // 새 문서에는 열린 편집기가 없다 — 되살린 초안이 있으면 스크립트가 다시 알린다.
        s.pending = false;
        let path = s.path.to_string_lossy().into_owned();
        let source = if s.mode == Mode::Preview {
            s.buffer.as_str()
        } else {
            ""
        };
        let html = preview::render(PreviewInput {
            theme: &theme,
            md_tr: &self.md_tr,
            file_path: &path,
            source,
            load_error: s.load_error.as_deref(),
            badge_label: &page::esc(self.tr.t("mdedit.agent_badge")),
        });
        let doc = page::document(&html, s, &theme, &self.tr, &snapshots);
        // 다시 그린 뒤 열 자리는 한 번만 쓴다.
        s.focus = None;
        if let Err(e) = host.call(
            "webview.set_url",
            json!({ "surface_id": surface_id, "url": doc }),
        ) {
            tracing::warn!("mdedit surface {surface_id}: webview.set_url failed: {e}");
        }
    }

    /// 세션 동작의 결과를 화면에 반영한다. 미리보기에서 버퍼가 바뀌면 다시 싣고, 원문
    /// 편집기에서는 부분 갱신으로 커서를 지킨다. `ack` 는 처리한 메시지 번호.
    pub(crate) fn present(&mut self, surface_id: u32, c: Changes, ack: Option<u64>) {
        let Some(s) = self.sessions.get(&surface_id) else {
            return;
        };
        if c.reload || (c.buffer && s.mode == Mode::Preview) {
            self.reload_page(surface_id);
            return;
        }
        let mut update = if c.chrome || c.buffer {
            page::chrome_update(s, &self.tr, &self.snapshots_for(surface_id), c.buffer)
        } else {
            json!({})
        };
        if let Some(seq) = ack {
            update["ack"] = json!(seq);
        }
        if update.as_object().is_some_and(|o| !o.is_empty()) {
            self.post(surface_id, &page::apply_msg(&update));
        }
    }

    pub(crate) fn on_navigation(&mut self, surface_id: u32, url: &str) {
        if let Some(parsed) = bridge::parse(url) {
            match parsed {
                Ok(msg) => self.on_message(surface_id, msg),
                Err(e) => tracing::warn!("mdedit surface {surface_id}: bad message: {e}"),
            }
            return;
        }
        // 본문 링크 — 렌더러가 `#tasty-nav:link:` 로 감싼 것. 주소창은 편집기에서 숨긴다.
        if let Some(preview::render::NavIntent::Link(dest)) =
            preview::render::parse_nav_fragment(url)
        {
            let base = self
                .sessions
                .get(&surface_id)
                .and_then(|s| s.path.parent().map(|p| p.to_path_buf()));
            match preview::classify(&dest, base.as_deref()) {
                Some(LinkClick::File(p)) if p.exists() => {
                    if let Some(host) = &self.host
                        && let Err(e) = host.call(
                            "file_handler.dispatch",
                            json!({ "path": p.to_string_lossy(), "depth": "deep", "origin_surface_id": surface_id }),
                        )
                    {
                        tracing::warn!("mdedit link dispatch failed: {e}");
                    }
                }
                Some(LinkClick::External(u)) => {
                    if let Err(e) = webbrowser::open(&u) {
                        tracing::warn!("mdedit external link open failed ({u}): {e}");
                    }
                }
                _ => {}
            }
        }
    }

    fn on_message(&mut self, surface_id: u32, msg: Message) {
        let Some(s) = self.sessions.get_mut(&surface_id) else {
            return;
        };
        if msg.seq != s.expect_seq {
            // 메시지 하나를 잃었다 — 이번 것을 버리고 원문 전체를 다시 받는다.
            tracing::warn!(
                "mdedit surface {surface_id}: message {} while expecting {} — resync",
                msg.seq,
                s.expect_seq
            );
            s.expect_seq = msg.seq + 1;
            let resend = s.mode == Mode::Source;
            self.post(
                surface_id,
                &page::apply_msg(&json!({ "ack": msg.seq, "resend": resend })),
            );
            return;
        }
        s.expect_seq += 1;
        let (op, payload) = if msg.op == "chunk" {
            match s.chunks.push(&msg.payload) {
                Ok(Some(whole)) => whole,
                Ok(None) => {
                    self.present(surface_id, Changes::default(), Some(msg.seq));
                    return;
                }
                Err(e) => {
                    tracing::warn!("mdedit surface {surface_id}: {e} — resync");
                    self.post(
                        surface_id,
                        &page::apply_msg(&json!({ "ack": msg.seq, "resend": true })),
                    );
                    return;
                }
            }
        } else {
            (msg.op, msg.payload)
        };
        let c = self.run_op(surface_id, &op, &payload);
        self.present(surface_id, c, Some(msg.seq));
    }

    fn run_op(&mut self, surface_id: u32, op: &str, p: &Value) -> Changes {
        let keep = self.retention();
        let ctx = Ctx {
            store: &self.store,
            keep,
        };
        let Some(s) = self.sessions.get_mut(&surface_id) else {
            return Changes::default();
        };
        let num = |k: &str| p.get(k).and_then(Value::as_u64).map(|v| v as usize);
        let mut resend = false;
        let changes = match op {
            "edit" => {
                let mut next = s.buffer.clone();
                match bridge::apply_splice(&mut next, p) {
                    Ok(()) => s.user_edit(next),
                    Err(e) => {
                        tracing::warn!("mdedit surface {surface_id}: {e} — resync");
                        resend = true;
                        Changes::default()
                    }
                }
            }
            "full" => {
                let t = p.get("t").and_then(Value::as_str).unwrap_or_default();
                s.user_edit(t.to_string())
            }
            "save" => s.save(&ctx, None),
            "blk" => {
                let text = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default();
                let then = match text("then") {
                    "item" => Then::NewItem,
                    "line" => Then::NewLine,
                    _ => Then::Nothing,
                };
                match num("i") {
                    Some(i) => s.preview_replace(i, text("o"), text("t"), then),
                    None => Changes::reload(),
                }
            }
            "unit" | "uins" => match (num("u"), unit_op(op, p)) {
                (Some(u), Some(o)) => {
                    s.preview_unit(u, p.get("h").and_then(Value::as_str).unwrap_or_default(), o)
                }
                _ => Changes::reload(),
            },
            "split" => {
                let text = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default();
                match num("i") {
                    Some(i) => s.preview_split(i, text("o"), text("t"), text("r")),
                    None => Changes::reload(),
                }
            }
            "ins" => {
                let t = p.get("t").and_then(Value::as_str).unwrap_or_default();
                let chain = p.get("c").and_then(Value::as_bool).unwrap_or(false);
                s.preview_insert(num("a"), t, chain)
            }
            "row" => match num("i") {
                Some(i) => s.preview_row(i, p.get("b").and_then(Value::as_bool).unwrap_or(true)),
                None => Changes::reload(),
            },
            "unitem" => match num("i") {
                Some(i) => s.preview_exit_item(i),
                None => Changes::reload(),
            },
            "pend" => s.set_pending(p.get("p").and_then(Value::as_bool).unwrap_or(false)),
            "task" => match (num("i"), num("n")) {
                (Some(i), Some(n)) => s.toggle_task(i, n),
                _ => Changes::default(),
            },
            "act" => {
                let a = p.get("a").and_then(Value::as_str).unwrap_or_default();
                let x = p.get("x").and_then(Value::as_str).unwrap_or_default();
                let idx = x.parse::<usize>().ok();
                match a {
                    "mode" => s.set_mode(if x == "source" {
                        Mode::Source
                    } else {
                        Mode::Preview
                    }),
                    "panel" => s.toggle_panel(if x == "review" {
                        Panel::Review
                    } else {
                        Panel::Snapshots
                    }),
                    "dismiss" => s.dismiss_banner(),
                    "keep" => idx.map_or_else(Changes::default, |i| s.review_keep(i)),
                    "keep_all" => s.review_keep_all(),
                    "revert" => idx.map_or_else(Changes::default, |i| s.review_revert(i, &ctx)),
                    "choose" => match x.split_once(':') {
                        Some((i, c)) => match (i.parse::<usize>(), Choice::parse(c)) {
                            (Ok(i), Some(c)) => s.choose(i, c),
                            _ => Changes::default(),
                        },
                        None => Changes::default(),
                    },
                    "resolve" => s.apply_resolution(),
                    "restore" => s.restore_snapshot(x, &ctx),
                    other => {
                        tracing::warn!("mdedit: unknown action '{other}'");
                        Changes::default()
                    }
                }
            }
            other => {
                tracing::warn!("mdedit: unknown op '{other}'");
                Changes::default()
            }
        };
        if resend {
            self.post(surface_id, &page::apply_msg(&json!({ "resend": true })));
        }
        changes
    }
}

/// 손잡이 메시지(`unit` · `uins`)를 연산으로 — 모르는 동작이면 `None`.
fn unit_op(op: &str, p: &Value) -> Option<UnitOp> {
    let text = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default();
    let num = |k: &str| p.get(k).and_then(Value::as_u64).map(|v| v as usize);
    if op == "uins" {
        return Some(UnitOp::Insert {
            text: text("t").to_string(),
            chain: p.get("c").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    Some(match text("a") {
        "up" => UnitOp::Up,
        "down" => UnitOp::Down,
        "dup" => UnitOp::Duplicate,
        "del" => UnitOp::Delete,
        "new" => UnitOp::NewItem,
        "fmt" => UnitOp::Format(Format::parse(text("f"))?),
        "move" => UnitOp::Move(match (num("bu"), num("bl")) {
            (Some(u), _) => Some(Entry::Unit(u)),
            (_, Some(k)) => Some(Entry::List(k)),
            _ => None,
        }),
        _ => return None,
    })
}

#[cfg(test)]
mod unit_op_tests {
    use super::*;

    #[test]
    fn handle_messages_map_to_unit_ops() {
        let op = |o: &str, p: Value| unit_op(o, &p);
        assert_eq!(op("unit", json!({"a": "up"})), Some(UnitOp::Up));
        assert_eq!(
            op("unit", json!({"a": "fmt", "f": "h2"})),
            Some(UnitOp::Format(Format::H2))
        );
        assert_eq!(op("unit", json!({"a": "fmt", "f": "nope"})), None);
        assert_eq!(
            op("unit", json!({"a": "move", "bl": 3})),
            Some(UnitOp::Move(Some(Entry::List(3))))
        );
        assert_eq!(
            op("unit", json!({"a": "move", "bu": 2})),
            Some(UnitOp::Move(Some(Entry::Unit(2))))
        );
        assert_eq!(op("unit", json!({"a": "move"})), Some(UnitOp::Move(None)));
        assert_eq!(
            op("uins", json!({"t": "x", "c": true})),
            Some(UnitOp::Insert {
                text: "x".into(),
                chain: true
            })
        );
        assert_eq!(op("unit", json!({"a": "bogus"})), None);
    }
}
