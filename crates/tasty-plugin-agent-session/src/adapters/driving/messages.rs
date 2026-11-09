//! 대화 문서가 받는 메시지 — 문서를 다시 싣지 않는 증분 갱신.
//!
//! plugin 은 `webview.post_message` 로 JSON 값만 보내고, 페이지는 `tasty:message` 이벤트로 받아
//! `type` 에 맞는 처리기를 부른다. 처리기는 이 문서가 실을 때 한 번 들어간다([`script_html`]).
//! 글은 `textContent` 로 넣고, `html` 필드는 plugin 이 이스케이프해 그린 조각만 싣는다.
//!
//! 문서를 새로 실으면 그 시점의 상태가 문서에 다 들어 있다. 처리기가 등록되기 전에 온 메시지는
//! 사라질 수 있으므로 메시지는 그 뒤의 변화만 나른다.

/// 메시지 처리기 — 대화 문서에 한 번 싣는다.
pub fn script_html() -> &'static str {
    SCRIPT
}

const SCRIPT: &str = r#"<script>(function(){
function byId(i){return document.getElementById(i);}
var H={
'activity.show':function(m){var a=byId('act');if(a)a.hidden=!m.show;},
'view.save':function(m){window.__asSaveView=!!m.on;},
'note.set':function(m){var n=byId('clearnote');if(!n)return;n.textContent=m.text==null?'':m.text;n.hidden=m.text==null;},
'queue.settle':function(m){
var q=byId('queue');
if(q){
if(m.staged!=null){var e=q.querySelector('.q.staged[data-n="'+m.staged+'"]');if(e)e.remove();}
Array.prototype.slice.call(q.querySelectorAll('.q:not(.staged)')).forEach(function(e){e.remove();});
var first=q.querySelector('.q.staged');
m.queued.forEach(function(v){var d=document.createElement('div');d.className='q';d.textContent=v;q.insertBefore(d,first);});
q.hidden=!q.querySelector('.q');
}
if(m.sent!=null){var act=byId('act');if(act)act.hidden=true;}
},
};
window.addEventListener('tasty:message',function(e){var m=e.detail;if(!m)return;if(window.__asApp&&window.__asApp.apply(m))return;var h=H[m.type];if(h)h(m);});
})();</script>"#;

/// 이 처리기 표가 받는 메시지 종류 — 대화 로그의 메시지는 페이지 앱(`client`)이 먼저 받는다.
#[cfg(test)]
pub const TYPES: &[&str] = &["activity.show", "view.save", "note.set", "queue.settle"];

/// 페이지 앱이 받는 메시지 종류. `queue.settle` 은 둘이 나눠 맡는다(로그 쪽은 앱, 대기열 칸은 처리기).
#[cfg(test)]
pub const APP_TYPES: &[&str] = &[
    "ask.set",
    "ask.error",
    "status.set",
    "status.copied",
    "turn.settle",
    "turn.overdue",
    "turn.auto",
    "turns.trim",
    "sys.line",
    "queue.settle",
    "subs.set",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// 처리기 표와 [`TYPES`] 가 같은 이름을 같은 수만큼 갖는다.
    #[test]
    fn every_known_type_has_one_handler() {
        for t in TYPES {
            assert_eq!(
                SCRIPT.matches(&format!("'{t}':function")).count(),
                1,
                "{t} 처리기가 하나가 아니다"
            );
        }
        assert_eq!(SCRIPT.matches("':function(m)").count(), TYPES.len());
    }

    /// 앱이 받는다고 한 종류는 앱의 `apply` 에 갈래가 있다.
    #[test]
    fn every_app_type_has_a_case_in_the_app() {
        let app = include_str!("../../../assets/app.js");
        for t in APP_TYPES {
            assert!(
                app.contains(&format!("case '{t}':")),
                "{t} 갈래가 앱에 없다"
            );
        }
        assert!(
            SCRIPT.contains("window.__asApp.apply(m)"),
            "앱이 먼저 받지 않는다"
        );
    }

    /// 처리기는 받은 글을 HTML 로 해석하지 않는다.
    #[test]
    fn text_fields_are_set_as_text() {
        assert!(!SCRIPT.contains("innerHTML=m.text"));
        assert!(!SCRIPT.contains("innerHTML=m.sent"));
    }
}
