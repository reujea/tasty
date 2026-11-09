//! 페이지 우클릭 → 호스트 네이티브 메뉴(`webview.context_menu`) — "터미널 ID 복사".
//!
//! 고른 글이 없는 자리의 우클릭은 WebKit 기본 메뉴(다시 싣기 등) 대신 호스트 메뉴를 연다. 고른 글이
//! 있거나 입력칸 안이면 복사 · 붙여넣기가 필요해 기본 메뉴를 둔다. 페이지는 plugin 에 직접 말할 수
//! 없어 다른 조작처럼 fragment 이동으로 우클릭 위치를 넘긴다.
//!
//! 복사는 호스트가 하고, 고른 결과(`webview.context_menu_selected`)가 이 plugin 에 온다. 호스트 토스트는
//! 겹친 webview 를 통째로 숨기므로 쓰지 않고, 우클릭한 자리에 "복사함" 을 잠깐 띄운다.

use crate::adapters::driving::url::has_mark;
use crate::adapters::driving::url::query_of;

const MARK: &str = "agent-ctx";

/// 메뉴에서 고른 결과 — 호스트가 소유 plugin 에만 보낸다.
pub const SELECTED_EVENT: &str = "webview.context_menu_selected";

/// 페이지의 알림 메시지 이름(`tasty:message` 의 `type`).
const NOTE_TYPE: &str = "ctx-note";

/// 모든 문서 끝에 붙는 우클릭 처리기. 같은 hash 는 통지가 오지 않아 시각을 함께 싣는다.
pub fn script() -> String {
    format!(
        r#"<script>(function(){{
document.addEventListener('contextmenu',function(e){{
  var t=e.target;
  if(t&&t.closest&&t.closest('textarea,input,[contenteditable="true"]'))return;
  if(window.getSelection&&String(window.getSelection()))return;
  e.preventDefault();
  location.hash='{MARK}='+Date.now()+'&x='+Math.round(e.clientX)+'&y='+Math.round(e.clientY);
}});
window.addEventListener('tasty:message',function(e){{
  var m=e.detail;
  if(!m||m.type!=='{NOTE_TYPE}')return;
  var d=document.createElement('div');
  d.textContent=m.text;
  d.style.cssText='position:fixed;z-index:2147483647;padding:2px 6px;border-radius:4px;font:12px system-ui,sans-serif;background:#2d3340;color:#e6e9ef;pointer-events:none';
  d.style.left=Math.max(0,Math.min(m.x,innerWidth-80))+'px';
  d.style.top=Math.max(0,Math.min(m.y,innerHeight-24))+'px';
  document.body.appendChild(d);
  setTimeout(function(){{d.remove();}},1500);
}});
}})();</script>"#
    )
}

/// 우클릭 위치(webview 안 논리 픽셀).
#[derive(Debug, PartialEq)]
pub struct Request {
    pub x: f64,
    pub y: f64,
}

/// 우리 표식이면 요청을 돌려준다. 좌표가 없으면 표식이어도 원점으로 본다 —
/// 메뉴가 엉뚱한 자리에 뜨더라도 우클릭이 조용히 사라지는 것보다 낫다.
pub fn parse(url: &str) -> Option<Request> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !has_mark(&fields, MARK) {
        return None;
    }
    let coord = |key: &str| {
        fields
            .iter()
            .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    };
    Some(Request {
        x: coord("x"),
        y: coord("y"),
    })
}

/// 호스트에 그 surface 의 메뉴를 연다.
pub fn open<H: crate::ports::host::HostCall>(host: &H, sid: u32, req: Request) {
    let params = serde_json::json!({ "surface_id": sid, "x": req.x, "y": req.y });
    if let Err(e) = host.call("webview.context_menu", params) {
        tracing::warn!("[agent_session] surface={sid} 우클릭 메뉴를 못 열었다: {e}");
    }
}

/// 고른 결과를 받으면 우클릭한 자리에 알림을 띄운다. 우리 surface 가 아니거나 모양이 틀리면 무시한다.
pub fn on_selected<H: crate::ports::host::HostCall>(
    host: &H,
    payload: &serde_json::Value,
    ours: impl Fn(u32) -> bool,
) {
    let Some(sid) = payload
        .get("surface_id")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
    else {
        tracing::warn!("[agent_session] {SELECTED_EVENT} 모양이 틀리다: {payload}");
        return;
    };
    if !ours(sid)
        || payload.get("item").and_then(serde_json::Value::as_str) != Some("copy_surface_id")
    {
        return;
    }
    if payload.get("copied").and_then(serde_json::Value::as_bool) != Some(true) {
        // 실패는 호스트가 토스트로 알렸다.
        return;
    }
    let at = |k: &str| {
        payload
            .get(k)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
    };
    crate::application::session_surface::patch(
        host,
        sid,
        &serde_json::json!({
            "type": NOTE_TYPE,
            "text": crate::domain::i18n::t("sup.line.copied"),
            "x": at("x"),
            "y": at("y"),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_right_click_carries_the_point() {
        let url = "about:blank#agent-ctx=1730000000000&x=12&y=340";
        assert_eq!(parse(url), Some(Request { x: 12.0, y: 340.0 }));
        // WebKit 이 `#` 를 인코딩해 넘기는 모양도 같다.
        assert_eq!(
            parse("about:blank%23agent-ctx=1&x=1&y=2"),
            Some(Request { x: 1.0, y: 2.0 })
        );
    }

    #[test]
    fn other_marks_are_not_ours() {
        assert_eq!(parse("about:blank#agent-cwd=1"), None);
        assert_eq!(parse("about:blank#agent-ctxx=1&x=1&y=1"), None);
        assert_eq!(parse("about:blank"), None);
    }

    #[test]
    fn the_script_leaves_fields_and_selections_to_the_native_menu() {
        let s = script();
        assert!(s.contains("'contextmenu'"));
        assert!(s.contains("textarea,input"));
        assert!(s.contains("getSelection()))return"));
        assert!(s.contains(&format!("'{MARK}='+Date.now()")));
        assert!(parse(&format!("about:blank#{MARK}=1&x=3&y=4")).is_some());
    }

    #[test]
    fn a_copied_selection_posts_a_note_at_the_click_point() {
        struct Rec(std::cell::RefCell<Vec<(String, serde_json::Value)>>);
        impl crate::ports::host::HostCall for Rec {
            fn call(
                &self,
                method: &str,
                params: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                self.0.borrow_mut().push((method.into(), params));
                Ok(serde_json::Value::Null)
            }
        }
        let rec = Rec(Default::default());
        let ok = serde_json::json!({ "surface_id": 9, "item": "copy_surface_id", "copied": true, "x": 5.0, "y": 6.0 });
        on_selected(&rec, &ok, |sid| sid == 9);
        {
            let calls = rec.0.borrow();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].0, "webview.post_message");
            let data = &calls[0].1["data"];
            assert_eq!(data["type"], NOTE_TYPE);
            assert_eq!(
                (data["x"].as_f64(), data["y"].as_f64()),
                (Some(5.0), Some(6.0))
            );
            assert!(data["text"].as_str().is_some_and(|t| !t.is_empty()));
        }
        // 다른 탭 · 실패 · 다른 항목은 알리지 않는다.
        on_selected(&rec, &ok, |_| false);
        on_selected(
            &rec,
            &serde_json::json!({ "surface_id": 9, "item": "copy_surface_id", "copied": false }),
            |_| true,
        );
        on_selected(
            &rec,
            &serde_json::json!({ "surface_id": 9, "item": "other", "copied": true }),
            |_| true,
        );
        assert_eq!(rec.0.borrow().len(), 1);
    }

    #[test]
    fn the_script_shows_the_note_it_is_sent() {
        let s = script();
        assert!(s.contains("'tasty:message'"));
        assert!(s.contains(&format!("m.type!=='{NOTE_TYPE}'")));
        assert!(s.contains("setTimeout"));
    }

    #[test]
    fn the_host_call_names_the_surface_and_the_point() {
        struct Rec(std::cell::RefCell<Vec<(String, serde_json::Value)>>);
        impl crate::ports::host::HostCall for Rec {
            fn call(
                &self,
                method: &str,
                params: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                self.0.borrow_mut().push((method.into(), params));
                Ok(serde_json::Value::Null)
            }
        }
        let rec = Rec(Default::default());
        open(&rec, 9, Request { x: 1.0, y: 2.0 });
        let calls = rec.0.borrow();
        assert_eq!(calls[0].0, "webview.context_menu");
        assert_eq!(
            calls[0].1,
            serde_json::json!({ "surface_id": 9, "x": 1.0, "y": 2.0 })
        );
    }
}
