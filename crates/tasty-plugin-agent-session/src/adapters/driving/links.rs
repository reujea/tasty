//! 대화 탭 답 글의 **링크를 눌러 연다** — 터미널(claude 탭)의 링크 클릭과 같은 대상이다.
//!
//! - 찾기: 페이지 스크립트가 답 요소의 글자에서 URL(`https://` · `http://` · `ftp://` · `file://`)과 경로
//!   (`/a/b` · `~/a` · `./a` · `../a` · 확장자가 있는 상대 경로 `src/main.rs`, 뒤의 `:줄[:칸]` 포함)를 찾아
//!   `<a class="lnk">` 로 감싼다. 답은 늘 `textContent` 로 들어가므로(`md_table` 과 같은 이유) HTML 을 해석하지
//!   않고 글 노드를 나눠 `createElement` · `textContent` 로 짓는다. 해석(`tastyLinkFind`)은 DOM 을 안 만지는
//!   순수 함수다.
//! - 누르기: 링크를 누르면 페이지가 hash 표식 [`OPEN_MARK`] 로 대상을 알린다(값은 누를 때마다 바뀐다).
//! - 열기: URL 은 호스트 `webview.open_external`(기본 브라우저), 경로는 작업 폴더 기준으로 풀어 **있을 때만**
//!   호스트 `file_handler.dispatch`(폴더는 탐색기, 파일은 그 형식의 handler) — 터미널 링크와 같은 길이다.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::adapters::driving::url::{has_mark, percent_decode, query_of};

/// 링크를 눌렀다 — `agent-open=<시각>&u=<대상>`.
pub const OPEN_MARK: &str = "agent-open";

/// 누른 링크가 가리키는 것.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// 브라우저 · 연결 프로그램으로 여는 URL.
    Url(String),
    /// 로컬 경로 — 아직 있는지 보지 않은 값이다.
    Path(PathBuf),
}

/// 링크 표식이면 누른 대상 글자를 돌려준다.
pub fn parse(url: &str) -> Option<String> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !has_mark(&fields, OPEN_MARK) {
        return None;
    }
    fields
        .iter()
        .find_map(|kv| kv.strip_prefix("u="))
        .map(percent_decode)
        .filter(|u| !u.trim().is_empty())
}

/// 대상 글자를 URL 또는 경로로 가른다. 상대 경로는 `cwd` 기준, `~` 는 `home` 기준이다. 풀 수 없으면 `None`.
pub fn resolve(raw: &str, cwd: Option<&str>, home: Option<&Path>) -> Option<Target> {
    let raw = raw.trim();
    let lower = raw.to_ascii_lowercase();
    if ["http://", "https://", "ftp://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return Some(Target::Url(raw.to_string()));
    }
    let path = match lower.strip_prefix("file://") {
        Some(_) => {
            let rest = &raw["file://".len()..];
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            percent_decode(rest)
        }
        None => raw.to_string(),
    };
    let path = strip_line_suffix(&path);
    let p = if let Some(rest) = path.strip_prefix("~/") {
        home?.join(rest)
    } else if path == "~" {
        home?.to_path_buf()
    } else if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        Path::new(cwd.filter(|c| !c.trim().is_empty())?).join(path)
    };
    Some(Target::Path(p))
}

/// 경로 뒤의 `:줄` · `:줄:칸` 을 뗀다 — 숫자 마디만 두 개까지.
fn strip_line_suffix(p: &str) -> &str {
    let mut s = p;
    for _ in 0..2 {
        match s.rsplit_once(':') {
            Some((head, tail))
                if !head.is_empty()
                    && !tail.is_empty()
                    && tail.bytes().all(|b| b.is_ascii_digit()) =>
            {
                s = head
            }
            _ => break,
        }
    }
    s
}

/// 링크 표식이면 그 대상을 열고 `true` — 우리 표식이 아니면 `false`.
pub fn open_clicked<H: crate::ports::host::HostCall>(
    host: &H,
    sid: u32,
    cwd: Option<&str>,
    url: &str,
) -> bool {
    let Some(raw) = parse(url) else {
        return false;
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match resolve(&raw, cwd, home.as_deref()) {
        Some(Target::Url(u)) => {
            match host.call(
                "webview.open_external",
                json!({ "surface_id": sid, "url": u }),
            ) {
                Ok(v) if v.get("opened").and_then(serde_json::Value::as_bool) == Some(false) => {
                    tracing::warn!("[agent_session] surface={sid}: OS 가 링크를 열지 않았다: {u}");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("[agent_session] surface={sid}: 링크 열기 실패 ({u}): {e}")
                }
            }
        }
        Some(Target::Path(p)) if p.exists() => {
            let params = json!({
                "path": p.to_string_lossy(),
                "depth": "deep",
                "origin_surface_id": sid,
                "user_navigation_url": url,
            });
            if let Err(e) = host.call("file_handler.dispatch", params) {
                tracing::warn!(
                    "[agent_session] surface={sid}: 경로 열기 실패 ({}): {e}",
                    p.display()
                );
            }
        }
        Some(Target::Path(p)) => {
            tracing::debug!(
                "[agent_session] surface={sid}: 없는 경로라 열지 않는다: {}",
                p.display()
            );
        }
        None => tracing::debug!("[agent_session] surface={sid}: 풀 수 없는 링크: {raw}"),
    }
    true
}

/// 페이지에 한 번 싣는 스타일과 스크립트. 문서 끝에서 지금 있는 답을 모두 한 번 훑는다.
pub fn script_html() -> String {
    format!(
        "<style>{CSS}</style><script>{JS}\n(function(){{var g=document.querySelectorAll('.got,.err');for(var i=0;i<g.length;i++)tastyLinks(g[i]);}})();</script>"
    )
}

const CSS: &str = ".got a.lnk,.err a.lnk{color:#7cacf8;text-decoration:underline;text-decoration-color:#3d5a80;text-underline-offset:2px;cursor:pointer}.got a.lnk:hover,.err a.lnk:hover{text-decoration-color:#7cacf8}";

/// `tastyLinkFind(s)` → `[{s,e,t}]`(글자 위치 · 대상) · `tastyLinks(el)` → 글 노드를 링크로 나눈다(한 번만,
/// `data-lnk`) · 누르면 `#agent-open=<시각>&u=<대상>`.
const JS: &str = r#"function tastyLinkFind(s){
var out=[],m,taken=[];
var END=/[.,;:!?]+$/;
function trim(t){t=t.replace(END,'');for(var pairs=[[')','('],[']','['],['}','{']],i=0;i<pairs.length;i++){var c=pairs[i][0],o=pairs[i][1];while(t.slice(-1)===c&&t.split(c).length>t.split(o).length)t=t.slice(0,-1);}return t.replace(END,'');}
var URL=/\b(?:https?|ftp|file):\/\/[^\s<>"'`\[\]{}|\\^]+/gi;
while((m=URL.exec(s))){var t=trim(m[0]);if(t.length>8){out.push({s:m.index,e:m.index+t.length,t:t});taken.push([m.index,m.index+m[0].length]);}}
var SEG="[^\\s<>\"'`\\[\\]{}|\\\\^():,;!?*]";
var PATH=new RegExp("(?:~|\\.{1,2})?/"+SEG+"+(?:/"+SEG+"*)*|[A-Za-z0-9_\\-\\u0080-\\uFFFF][A-Za-z0-9._\\-\\u0080-\\uFFFF]*(?:/[A-Za-z0-9._\\-\\u0080-\\uFFFF]+)*/[A-Za-z0-9._\\-\\u0080-\\uFFFF]*\\.[A-Za-z][A-Za-z0-9]{0,7}","g");
var LINE=/^:\d+(?::\d+)?/;
while((m=PATH.exec(s))){
  var a=m.index,t=m[0];
  var prev=a>0?s.charAt(a-1):'';
  if(prev&&/[A-Za-z0-9_.~\/\-:@\u0080-￿]/.test(prev))continue;
  if(taken.some(function(r){return a<r[1]&&a+t.length>r[0];}))continue;
  var rest=s.slice(a+t.length),ln=LINE.exec(rest);
  t=trim(t);
  if(t.charAt(0)==='/'&&t.indexOf('/',1)<0)continue;
  if(t==='~'||t==='.'||t==='..'||/\/$/.test(t)&&t.length<3)continue;
  if(ln&&t===m[0])t+=ln[0];
  out.push({s:a,e:a+t.length,t:t});
}
out.sort(function(x,y){return x.s-y.s;});
return out;
}
function tastyLinks(el){
if(!el||el.getAttribute('data-lnk'))return;
el.setAttribute('data-lnk','1');
var w=document.createTreeWalker(el,NodeFilter.SHOW_TEXT,null),nodes=[],n;
while((n=w.nextNode()))if(!(n.parentNode&&n.parentNode.closest&&n.parentNode.closest('a')))nodes.push(n);
nodes.forEach(function(node){
  var s=node.nodeValue,hits=tastyLinkFind(s);
  if(!hits.length)return;
  var f=document.createDocumentFragment(),at=0;
  hits.forEach(function(h){
    if(h.s<at)return;
    if(h.s>at)f.appendChild(document.createTextNode(s.slice(at,h.s)));
    var a=document.createElement('a');
    a.className='lnk';a.textContent=s.slice(h.s,h.e);a.setAttribute('data-u',h.t);a.title=h.t;
    f.appendChild(a);at=h.e;
  });
  if(at<s.length)f.appendChild(document.createTextNode(s.slice(at)));
  node.parentNode.replaceChild(f,node);
});
}
document.addEventListener('click',function(e){
var a=e.target&&e.target.closest&&e.target.closest('a.lnk');
if(!a)return;
e.preventDefault();
if(window.getSelection&&String(window.getSelection()).length)return;
location.hash='agent-open='+Date.now()+'&u='+encodeURIComponent(a.getAttribute('data-u')||'');
});"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_carries_the_clicked_target() {
        assert_eq!(
            parse("about:blank#agent-open=1792570000000&u=https%3A%2F%2Fexample.com%2Fa%3Fb%3D1"),
            Some("https://example.com/a?b=1".into())
        );
        assert_eq!(parse("about:blank#agent-open=1&u="), None);
        assert_eq!(parse("about:blank#agent-copy-cwd=1"), None);
    }

    #[test]
    fn urls_open_in_the_browser_and_file_urls_become_paths() {
        assert_eq!(
            resolve("https://example.com/x", None, None),
            Some(Target::Url("https://example.com/x".into()))
        );
        assert_eq!(
            resolve("mailto:a@b.com", None, None),
            Some(Target::Url("mailto:a@b.com".into()))
        );
        assert_eq!(
            resolve("file:///Users/me/%ED%95%9C%20a/b.md", None, None),
            Some(Target::Path("/Users/me/한 a/b.md".into()))
        );
        assert_eq!(
            resolve("file://localhost/tmp/x", None, None),
            Some(Target::Path("/tmp/x".into()))
        );
    }

    #[test]
    fn paths_resolve_against_the_working_folder_and_home_and_drop_line_numbers() {
        let home = Path::new("/Users/me");
        assert_eq!(
            resolve(&["src/main.rs", ":42:7"].concat(), Some("/w/p"), Some(home)),
            Some(Target::Path("/w/p/src/main.rs".into()))
        );
        assert_eq!(
            resolve("../q/a.toml", Some("/w/p"), Some(home)),
            Some(Target::Path("/w/p/../q/a.toml".into()))
        );
        assert_eq!(
            resolve("~/dev/x", Some("/w/p"), Some(home)),
            Some(Target::Path("/Users/me/dev/x".into()))
        );
        assert_eq!(
            resolve("/etc/hosts:3", None, None),
            Some(Target::Path("/etc/hosts".into()))
        );
        assert_eq!(
            resolve("src/a.rs", None, Some(home)),
            None,
            "작업 폴더가 없으면 상대 경로를 못 푼다"
        );
        assert_eq!(resolve("~/x", None, None), None);
        assert_eq!(strip_line_suffix("a:b"), "a:b");
        assert_eq!(strip_line_suffix(":12"), ":12");
    }

    /// 열기는 호스트가 한다 — URL 은 `webview.open_external`, 있는 경로는 `file_handler.dispatch`, 없는 경로는
    /// 아무것도 부르지 않는다.
    #[test]
    fn a_click_calls_the_host_method_for_its_kind() {
        #[derive(Default)]
        struct Rec(std::cell::RefCell<Vec<(String, serde_json::Value)>>);
        impl crate::ports::host::HostCall for Rec {
            fn call(
                &self,
                method: &str,
                p: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                self.0.borrow_mut().push((method.to_string(), p));
                Ok(serde_json::json!({}))
            }
        }
        let host = Rec::default();
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("as-links-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cwd = dir.to_str();
        assert!(open_clicked(
            &host,
            7,
            cwd,
            "about:blank#agent-open=1&u=https%3A%2F%2Fx.org"
        ));
        assert!(open_clicked(&host, 7, cwd, "about:blank#agent-open=2&u=."));
        assert!(open_clicked(
            &host,
            7,
            cwd,
            "about:blank#agent-open=3&u=nope%2Fmissing.rs"
        ));
        assert!(!open_clicked(&host, 7, cwd, "about:blank#agent-copy-cwd=4"));
        let calls = host.0.borrow();
        let methods: Vec<&str> = calls.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(methods, ["webview.open_external", "file_handler.dispatch"]);
        assert_eq!(calls[0].1["url"], "https://x.org");
        assert_eq!(calls[0].1["surface_id"], 7);
        assert_eq!(calls[1].1["origin_surface_id"], 7);
        assert!(
            calls[1].1["path"]
                .as_str()
                .unwrap()
                .starts_with(dir.to_str().unwrap())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_links_are_built_without_parsing_html_and_answers_link_them() {
        assert!(
            !JS.contains("innerHTML")
                && !JS.contains("insertAdjacentHTML")
                && !JS.contains("outerHTML")
        );
        assert!(
            !JS.contains("</script"),
            "스크립트 안에 닫는 태그가 있으면 문서가 끊긴다"
        );
        assert!(JS.contains(&format!("location.hash='{OPEN_MARK}='")));
        let app = include_str!("../../../assets/app.js");
        assert!(
            app.contains("if (window.tastyLinks) window.tastyLinks(g);"),
            "답 요소에 링크를 안 붙인다"
        );
    }
}
