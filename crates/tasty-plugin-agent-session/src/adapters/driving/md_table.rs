//! 대화 탭 답 글의 **마크다운 표를 표로 그린다**.
//!
//! 답은 늘 `textContent` 로 들어간다 — `innerHTML` 이면 답에 든 태그가 실행된다(`settle_turn_script` ·
//! `stream::append_script`). 그래서 HTML 을 서버에서 만들지 않고, 페이지 스크립트가 답 요소의 **글자**에서
//! 표 덩어리(머리 줄 + 구분 줄 `|---|` + 몸 줄)를 찾아 `createElement` · `textContent` 로 `<table>` 을 짓는다.
//! 표가 아닌 글은 그대로 글자로 남는다(`white-space: pre-wrap`).
//!
//! - 정렬: 구분 줄의 `:---`(왼쪽) · `:---:`(가운데) · `---:`(오른쪽)
//! - 셀 안의 `\|` 는 구분자가 아니다
//! - 칸 수가 모자란 줄은 빈 칸으로 채운다 — 넘치면 자른다(머리 줄이 칸 수를 정한다)
//!
//! 해석(`tastyMdParse`)은 DOM 을 안 만지는 순수 함수라 따로 시험할 수 있고, 그리기(`tastyTables`)는
//! 답 요소 하나를 받아 한 번만 바꾼다(`data-tbl`).
//!
//! **코드 펜스(```` ``` ```` · `~~~`) 안은 칸을 고정한다.** 구성도는 동아시아 폭(W/F = 2칸) 기준으로 열을
//! 맞춰 오는데, 본문 글꼴(`ui-monospace` · Menlo)에는 한글 글리프가 없어 대체 글꼴로 그려지고 그 폭이 영문 두
//! 칸과 달라 `│` 열이 어긋났다. 그래서 펜스 안의 비ASCII 글자를 한 자씩 `span` 으로 감싸 W/F 는 `2ch`, 나머지
//! (박스 선 · `·` · `←` 처럼 모호 폭인 것 포함)는 `1ch` 로 못 박는다 — 글꼴이 무엇이든 칸이 맞는다. 감싸도
//! 글자는 그대로라 복사한 원문이 안 바뀐다. 결합 문자(변이 선택자 · ZWJ · 조합형 한글 중성·종성 등)는 앞 글자의
//! 칸에 붙인다. 펜스 안 줄은 표로 읽지 않는다.

/// 페이지에 한 번 싣는 스타일과 스크립트. 문서 끝에서 지금 있는 답을 모두 한 번 훑는다.
pub fn script_html() -> String {
    format!(
        "<style>{CSS}</style><script>{JS}\n(function(){{var g=document.querySelectorAll('.got');for(var i=0;i<g.length;i++)tastyTables(g[i]);}})();</script>"
    )
}

const CSS: &str = "\
.got .w1,.got .w2{display:inline-block;text-indent:0}.got .w1{width:1ch}.got .w2{width:2ch}\
.got table.md{border-collapse:collapse;margin:6px 0;text-indent:0;font-variant-numeric:tabular-nums;white-space:normal}\
.got table.md th,.got table.md td{border:1px solid #3a3f4b;padding:2px 8px;vertical-align:top;white-space:pre-wrap}\
.got table.md th{background:#262a33;font-weight:600}\
.got table.md tbody tr:nth-child(even) td{background:#1d2027}";

/// 해석 + 그리기. `tastyMdParse(text)` → `[{t:"글"} | {head:[...], align:[...], rows:[[...]]}]`.
pub const JS: &str = r#"
function tastyMdCells(l){
  var s=l.trim();
  if(s.charAt(0)==='|')s=s.slice(1);
  if(s.length&&s.charAt(s.length-1)==='|'&&s.charAt(s.length-2)!=='\\')s=s.slice(0,-1);
  var out=[],cur='';
  for(var i=0;i<s.length;i++){
    var c=s.charAt(i);
    if(c==='\\'&&s.charAt(i+1)==='|'){cur+='|';i++;continue;}
    if(c==='|'){out.push(cur.trim());cur='';continue;}
    cur+=c;
  }
  out.push(cur.trim());
  return out;
}
function tastyMdIsSep(l){
  if(l.indexOf('-')<0||l.indexOf('|')<0)return false;
  var c=tastyMdCells(l);
  for(var i=0;i<c.length;i++){if(!/^:?-+:?$/.test(c[i]))return false;}
  return c.length>0;
}
function tastyMdIsRow(l){return l.indexOf('|')>=0&&l.trim().length>0;}
var TASTY_FENCE=/^ {0,3}(```|~~~)/;
function tastyMdParse(text){
  var lines=text.split('\n'),out=[],buf=[],i=0,fen=false;
  function flush(){if(buf.length){out.push({t:buf.join('\n')});buf=[];}}
  while(i<lines.length){
    if(TASTY_FENCE.test(lines[i]))fen=!fen;
    else if(!fen&&i+1<lines.length&&tastyMdIsRow(lines[i])&&tastyMdIsSep(lines[i+1])){
      var head=tastyMdCells(lines[i]),n=head.length;
      var align=tastyMdCells(lines[i+1]).map(function(c){
        var l=c.charAt(0)===':',r=c.charAt(c.length-1)===':';
        return l&&r?'center':(r?'right':'left');
      });
      while(align.length<n)align.push('left');
      var rows=[];i+=2;
      while(i<lines.length&&tastyMdIsRow(lines[i])&&!tastyMdIsSep(lines[i])){
        var r=tastyMdCells(lines[i]);
        while(r.length<n)r.push('');
        rows.push(r.slice(0,n));i++;
      }
      flush();
      out.push({head:head,align:align.slice(0,n),rows:rows});
      continue;
    }
    buf.push(lines[i]);i++;
  }
  flush();
  return out;
}
var TASTY_WIDE=/[\u1100-\u115F\u231A\u231B\u2329\u232A\u23E9-\u23EC\u23F0\u23F3\u25FD\u25FE\u2614\u2615\u2648-\u2653\u267F\u2693\u26A1\u26AA\u26AB\u26BD\u26BE\u26C4\u26C5\u26CE\u26D4\u26EA\u26F2\u26F3\u26F5\u26FA\u26FD\u2705\u270A\u270B\u2728\u274C\u274E\u2753-\u2755\u2757\u2795-\u2797\u27B0\u27BF\u2B1B\u2B1C\u2B50\u2B55\u2E80-\u303E\u3041-\u33FF\u3400-\u4DBF\u4E00-\u9FFF\uA000-\uA4CF\uA960-\uA97F\uAC00-\uD7A3\uF900-\uFAFF\uFE10-\uFE19\uFE30-\uFE6F\uFF00-\uFF60\uFFE0-\uFFE6\u{1F004}\u{1F0CF}\u{1F18E}\u{1F191}-\u{1F19A}\u{1F200}-\u{1F251}\u{1F300}-\u{1F64F}\u{1F680}-\u{1F6FF}\u{1F7E0}-\u{1F7EB}\u{1F90C}-\u{1F9FF}\u{1FA70}-\u{1FAFF}\u{20000}-\u{3FFFD}]/u;
var TASTY_JOIN=/[\u0300-\u036F\u1160-\u11FF\u200B-\u200F\u20D0-\u20FF\uFE00-\uFE0F\u{1F3FB}-\u{1F3FF}\u{E0020}-\u{E007F}\u{E0100}-\u{E01EF}]/u;
function tastyMdRuns(line){
  var out=[],zwj=false;
  for(var ch of line){
    var last=out[out.length-1];
    if(last&&last.w&&(zwj||TASTY_JOIN.test(ch))){last.t+=ch;zwj=ch==='\u200D';continue;}
    zwj=false;
    if(ch.charCodeAt(0)<128){if(last&&!last.w)last.t+=ch;else out.push({t:ch});continue;}
    out.push({t:ch,w:TASTY_WIDE.test(ch)?2:1});
  }
  return out;
}
function tastyMdText(el,text){
  var lines=text.split('\n'),plain='',fen=false;
  for(var i=0;i<lines.length;i++){
    var l=lines[i]+(i+1<lines.length?'\n':'');
    if(TASTY_FENCE.test(lines[i])){fen=!fen;plain+=l;continue;}
    if(!fen||!/[^\x00-\x7f]/.test(l)){plain+=l;continue;}
    if(plain){el.appendChild(document.createTextNode(plain));plain='';}
    tastyMdRuns(l).forEach(function(r){
      if(!r.w){el.appendChild(document.createTextNode(r.t));return;}
      var s=document.createElement('span');s.className='w'+r.w;s.textContent=r.t;el.appendChild(s);
    });
  }
  if(plain)el.appendChild(document.createTextNode(plain));
}
function tastyTables(el){
  if(!el||el.getAttribute('data-tbl'))return;
  var text=el.textContent;
  var wide=/^ {0,3}(```|~~~)/m.test(text)&&/[^\x00-\x7f]/.test(text);
  if(!wide&&(text.indexOf('|')<0||text.indexOf('-')<0))return;
  var parts=tastyMdParse(text),has=wide;
  for(var k=0;!has&&k<parts.length;k++){if(parts[k].head)has=true;}
  if(!has)return;
  el.setAttribute('data-tbl','1');
  el.textContent='';
  parts.forEach(function(p){
    if(!p.head){tastyMdText(el,p.t);return;}
    var tb=document.createElement('table');tb.className='md';
    var th=document.createElement('thead'),tr=document.createElement('tr');
    p.head.forEach(function(h,j){var c=document.createElement('th');c.textContent=h;c.style.textAlign=p.align[j];tr.appendChild(c);});
    th.appendChild(tr);tb.appendChild(th);
    var bd=document.createElement('tbody');
    p.rows.forEach(function(r){var row=document.createElement('tr');r.forEach(function(v,j){var c=document.createElement('td');c.textContent=v;c.style.textAlign=p.align[j];row.appendChild(c);});bd.appendChild(row);});
    tb.appendChild(bd);el.appendChild(tb);
  });
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// 답 글은 태그로 해석되면 안 된다 — 표도 `textContent` 로 짓는다.
    #[test]
    fn the_table_is_built_without_parsing_html() {
        assert!(
            !JS.contains("innerHTML")
                && !JS.contains("outerHTML")
                && !JS.contains("insertAdjacentHTML")
        );
        assert!(JS.contains("c.textContent=h") && JS.contains("c.textContent=v"));
    }

    #[test]
    fn the_page_script_runs_once_over_existing_answers_and_closes_safely() {
        let s = script_html();
        assert!(s.contains("querySelectorAll('.got')"));
        assert!(
            !JS.contains("</script"),
            "스크립트 안에 닫는 태그가 있으면 문서가 끊긴다"
        );
    }

    /// 펜스 안 글자 칸도 `textContent` 로 짓고, 칸이 답의 내어쓰기(`text-indent:-2ch`)를 물려받지 않는다 —
    /// 물려받으면 `inline-block` 안의 글자가 왼쪽 칸으로 밀려 겹친다.
    #[test]
    fn fenced_glyph_cells_are_text_and_do_not_inherit_the_hanging_indent() {
        assert!(JS.contains("s.textContent=r.t"));
        assert!(CSS.contains(".got .w1,.got .w2{display:inline-block;text-indent:0}"));
        assert!(CSS.contains(".got .w1{width:1ch}") && CSS.contains(".got .w2{width:2ch}"));
        assert!(JS.contains("else if(!fen&&"), "펜스 안 줄을 표로 읽는다");
    }

    /// 답이 들어가는 메시지마다 표를 짓는다 — 하나라도 빠지면 그 길로 온 답만 표가 안 된다.
    #[test]
    fn every_answer_message_builds_tables() {
        let app = include_str!("../../../assets/app.js");
        assert!(
            app.contains("if (window.tastyTables) window.tastyTables(g);"),
            "답 요소에 표를 안 짓는다"
        );
        assert!(
            app.contains(r#"<${Answer} cls="got" text=${t.got} />"#),
            "답 턴이 표를 거치지 않는다"
        );
        assert!(
            app.contains(r#"<${Answer} cls="err" text=${t.error} />"#),
            "실패 턴이 표를 거치지 않는다"
        );
        assert!(
            app.contains("<${Answer} cls=${t.error != null"),
            "이전 대화가 표를 거치지 않는다"
        );
        let page = include_str!("view.rs");
        assert!(
            page.contains("h.push_str(&crate::adapters::driving::md_table::script_html());"),
            "표 스크립트를 안 싣는다"
        );
    }
}
