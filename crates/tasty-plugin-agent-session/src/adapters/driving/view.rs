//! 대화 탭의 화면 — 세션이 있으면 대화창, 없으면 시작 폼을 HTML 로 그리고 상태줄 값을 만든다.
//!
//! 여기는 그리기만 한다. 그린 문서를 호스트에 싣는 것은 `session_surface`, 문서가 낸 navigation
//! 주소를 읽는 것은 `url`, 그리는 대상인 대화 상태는 `convo` 다.

pub(crate) mod clear_button;
mod start_form;
mod start_style;
pub(crate) mod table;

use crate::adapters::driving::html::escape;
use crate::adapters::driving::url::*;
use crate::domain::convo::*;

/// 대화를 HTML 로 만든다.
///
/// 입력 폼은 JS 로 fragment 를 만들어 이동한다 — 그 이동이 곧 plugin 에게
/// 가는 신호다.
pub fn render(convo: &Convo, busy: bool) -> String {
    // 이어진 세션이 없으면 **대화창 대신 시작 폼**이다. 빈 대화창을 주면
    // 입력해도 "세션이 없다" 만 나와, 사용자가 무엇을 해야 할지 모른다.
    if convo.session_id.is_none() {
        return start_form::render_start_with(
            &convo.models,
            &convo.known_agents,
            &convo.attachable,
            &convo.profiles,
            &convo.cwd_pick,
            convo.start_error.as_deref(),
            &start_form::Extras {
                roles: &convo.roles,
                manage_note: convo.manage_note.as_ref(),
                this_tab: convo.tab_id,
                field_pick: convo.field_pick.as_ref().map(|(f, v)| (*f, v.as_str())),
            },
        );
    }
    let mut h = String::with_capacity(4096);
    h.push_str(
        r#"<!doctype html><meta charset="utf-8">
<style>
 :root{color-scheme:dark light}
 /* 줄 높이는 터미널의 Claude Code 와 맞춘다 — 터미널 셀은 글자 크기의 약 1.2 배다. 1.6 이면
    답의 빈 줄(문단 · 표 · 목록 사이)마다 한 줄씩 더 벌어져 같은 답이 터미널보다 1/3 성기게
    보였다(사용자 보고 2026-10-21). */
 body{font:13px/1.3 ui-monospace,SFMono-Regular,Menlo,monospace;margin:0;
      background:#16181d;color:#c8ccd4;display:flex;flex-direction:column;height:100vh}
 #log{flex:1;overflow-y:auto;padding:16px}
 /* ── 턴 ──────────────────────────────────────────────────
    TUI 벤치마킹(2026-10-02 · opencode 211k·gemini-cli 107k·goose 55k·
    crush 28k·gum 24k 의 **렌더 코드 리터럴** 실측):
      · `> ` 사용자 프롬프트 — 5개 중 4개 공통
      · 좌측 세로줄로 블록 구분 — opencode `┃`·crush `▌` (다수파)
      · 응답 마커는 소수파(opencode `▣`·gemini-cli `✦`). 여기서는
        기준점을 Claude Code 로 잡아 `⏺` 를 쓴다(사용자 지정). */
 .turn{margin-bottom:16px;border-left:2px solid #2a2e37;padding-left:12px}
 .turn.pending{border-left-color:#5ac17d}
 .sent{color:#9aa3b2;white-space:pre-wrap;margin-bottom:8px}
 .sent::before{content:"> ";color:#5ac17d}
 /* 응답은 마커 열(2ch)을 비워 두고 그 자리에 `⏺` 를 건다 — 줄이
    넘어가도 본문이 마커 아래로 흐르지 않는다(hanging indent). */
 .got{color:#e6e9ef;white-space:pre-wrap;padding-left:2ch;text-indent:-2ch}
 .got::before{content:"\23FA  ";color:#5ac17d}
 .err{color:#e06c75;white-space:pre-wrap;padding-left:2ch;text-indent:-2ch}
 .err::before{content:"\00D7  ";color:#e06c75}
 .empty{color:#5a6070;padding:24px 0}
 .empty::before{content:"\23FA  ";color:#3a4150}
 /* 대기 칸 — 턴이 도는 동안 보낸 말. 입력창 바로 위에 쌓인다(Claude Code 와 같은 자리). */
 /* 대기 칸은 높이에 상한을 두고 그 안에서 스크롤한다 — 상한이 없으면 긴 말이 쌓일 때 대화 로그가
    0 까지 줄고 상태줄이 화면 위로 밀려 나갔다(사용자 보고 2026-10-21). 상태줄 · 입력칸은 줄지 않는다. */
 #queue{padding:6px 16px;border-top:1px solid #2a2e37;color:#9aa3b2;
        max-height:30vh;overflow-y:auto;flex-shrink:0}
 #log{min-height:0}
 .status,form{flex-shrink:0}
 #queue .qh{color:#5a6070;margin-bottom:4px}
 #queue .q{white-space:pre-wrap;padding-left:2ch;text-indent:-2ch}
 #queue .q::before{content:"\203A ";color:#d8a33c}
 #queue .q.staged{opacity:.6}
 /* ── 입력창 ───────────────────────────────────────────────
    **좌측 세로줄**(다수파). 둥근 박스 `╭─╮` 는 gemini-cli 하나뿐이라
    관행이 아니다 — 담백하고 긴 대화에서 시선을 덜 빼앗는다. */
 form{display:flex;align-items:center;gap:0;padding:10px 16px;
      border-top:1px solid #2a2e37;background:#1a1d23}
 .bar{color:#5ac17d;padding-right:10px;user-select:none;font-weight:700}
 .caret{color:#5ac17d;padding-right:6px;user-select:none}
 /* 입력은 `textarea` 다 — `input` 은 줄바꿈을 **담지 못한다**.
    Enter=전송 / Shift+Enter=줄바꿈(TUI·채팅 공통 관행).
    높이는 내용에 맞춰 스크립트가 늘린다(최대 10줄). */
 textarea{flex:1;background:transparent;border:0;color:#e6e9ef;
       padding:4px 0;font:inherit;resize:none;overflow-y:auto;
       line-height:1.3;max-height:13em;cursor:text}
 textarea:focus{outline:none}
 /* 입력칸 위에서는 **텍스트 커서**를 보여 준다 — 어디를 눌러야 쓸 수
    있는지가 모양으로 드러난다. 폼 전체에 걸어 캐럿·세로줄 위에서도
    같은 커서가 나온다(그쪽을 눌러도 입력으로 들어간다). */
 form{cursor:text}
 form button{cursor:pointer}
 textarea::placeholder{color:#4a515e}
 /* 여러 줄이 되면 세로줄·캐럿이 가운데 뜨지 않게 위로 붙인다. */
 form{align-items:flex-start}
 .bar,.caret{padding-top:4px}
 button{background:transparent;border:0;color:#5a6070;padding:4px 8px;
        font:inherit;cursor:pointer}
 button:hover:not(:disabled){color:#c8ccd4}
 button:disabled{opacity:.3;cursor:default}
 .got.stopped{color:#7d8694}
 .got.stopped::before{content:"\25A0  ";color:#525a68}
 button.stop{color:#e06c75;display:none}
 button.stop:hover{color:#ff8a94}
 body:has(.turn.pending) button.stop,body:has(#act:not([hidden])) button.stop{display:inline-block}
 .busy{color:#d8a33c;padding:0 16px 10px}
 .startwarn{color:#d8a33c;padding:0 16px 8px;white-space:pre-wrap}
 /* 보낸 직후의 **낙관적 턴** — 아직 답이 없다.
    보낸 말은 확정된 턴과 같은 모양으로 두고, 답 자리에만 진행 표시를
    둔다. 그래야 답이 오면 그 자리가 그대로 채워진 것처럼 이어진다. */
 /* ── 진행 표시 ────────────────────────────────────────────
    **Claude Code 실물 4프레임**(`·|·` `·/·` `·—·` `·\·`).

    **바이너리에서 실측했다**(2026-10-06 · `claude` 2.1.285):
    `["\xB7|\xB7","\xB7/\xB7","\xB7\u2014\xB7","\xB7\\\xB7"]` — 가운데 글자만
    돌고 양옆 `·` 는 고정이다. 완료 표시는 `·✔︎·` 로 같은 자리를 쓴다.

    ⚠️ **종전엔 브레일 10프레임이었다** — opencode·gemini-cli·gum 의
    공통 관행이라 골랐으나, 기준점을 **Claude Code 로 잡으면 틀린
    선택**이다(사용자 지정). 관행이 둘일 때는 *누구를 닮을 것인가* 가
    먼저고, 그 답이 정해지면 다수결은 근거가 아니다.

    ⚠️ JS 없이 돈다 — 키프레임이 `content` 를 프레임마다 갈아 끼운다
    (문서를 다시 실어도 서버가 같은 모양을 내므로 이어진다).

    ⚠️ **글리프 폭에 기대지 않는다.** 종전에는 네 프레임을 한 줄로 이어
    `translateX(-3ch)` 씩 밀었는데, `·`·`—` 가 이 폰트에서 1ch 가 아니라
    창이 글리프와 어긋났다 — 점 하나만 깜빡이거나 비어 보였다(실측
    2026-10-12 · WKWebView). `content` 교체는 폭과 무관하게 한 프레임을
    통째로 보인다(같은 날 같은 webview 에서 4프레임이 도는 것을 확인). */
 .pending .got{color:#7d8694}
 .pending .got::before{content:"\23FA  ";color:#3a4150}
 .spin{color:#5ac17d}
 .spin::before{content:"\B7|\B7";animation:spinframes .64s infinite}
 @keyframes spinframes{0%{content:"\B7|\B7"}25%{content:"\B7/\B7"}
       50%{content:"\B7\2014\B7"}75%{content:"\B7\\\B7"}}
 .think{color:#5a6070;margin-left:6px}
 /* 하단 상태줄 — 폼 바로 위에 붙는다. 대화가 길어져도 늘 보인다(`#log` 만 스크롤한다).
    모양은 `statusline::script_html` 이 문서 끝에 싣는다. */
 .older{color:#5a6070;margin-bottom:16px}
 .older .oturn{opacity:.7;margin-bottom:12px;border-left:2px solid #2a2e37;padding-left:12px}
 .older .more{color:#8b93a1;display:inline-block;margin-bottom:12px}
 /* `hidden` 속성은 위 display 에 진다 — 다 펼친 뒤 (0) 링크가 남았다(실측 2026-10-16). */
 .older .more[hidden]{display:none}
</style>
<div id="log">
"#,
    );
    // 로그는 페이지 앱(`client`)이 상태 JSON 으로 그린다.
    h.push_str("</div>");
    h.push_str(&crate::application::activity::row_html());
    // 세션이 기다리는 물음(권한 확인 · 질문) — 답하는 카드. 없으면 숨긴 자리만 둔다(`prompts`).
    h.push_str(r#"<div id="askbar"></div>"#);

    // **상태줄** — 대화창 하단, 입력 폼 바로 위.
    //
    // 여기 싣는 것은 전부 `Convo` 가 **이미 들고 있는 값**이다. 데몬에
    // 물어보면 매 렌더마다 왕복이 생기고, 이 surface 는 문서를 통째로
    // 다시 실어야 갱신되므로(`webview.set_url` 이 유일한 수단) 그 비용이
    // 화면 깜빡임으로 나타난다.
    h.push_str(r#"<div id="statusbar"></div>"#);
    h.push_str(&crate::domain::send_queue::queue_html(&convo.queued));
    h.push_str(&clear_button::note_html());
    if let Some(w) = convo.start_warning.as_deref() {
        // 위와 같은 축 — `String` 쓰기는 실패하지 않는다.
        h.push_str(&format!(
            r#"<div class="startwarn">{}</div>"#,
            escape(&crate::domain::i18n::t1("sup.chat.start_warning", w))
        ));
    }
    h.push_str(&crate::domain::commands::menu_html());
    // 하위 에이전트 패널 — 창 오른쪽에 붙고, 새 서브에이전트가 보이면 열린다(`subagents`).
    h.push_str(crate::adapters::driven::subagents::STYLE);
    h.push_str(crate::domain::sys_note::STYLE);
    h.push_str(r#"<div id="subpanelbar"></div>"#);

    // JS 를 쓰지 않는다 — 이 kind 의 webview 는 **기본이 sandbox(JS off)** 이고
    // 그 설정은 html·markdown kind 만 바꿀 수 있다(`webview_settings_plugin_id`).
    //
    //  **GET 제출은 페이지를 떠난다** — `action="#"` 이어도 그렇다.
    //
    // 종전 주석은 *"빈 fragment 로 두면 제자리에 머물면서 쿼리만
    // 바뀐다"* 고 적었는데 **틀렸다**(실측 2026-09-30). 브라우저는 그것을
    // `formSubmitted` 로 처리해 `about:blank?t=…` 로 **실제 이동**하고,
    // 그 빈 URL 이 곧 흰 화면이다.
    //
    // ⇒ **스크립트가 제출을 가로채 `location.hash` 로 보낸다**. fragment
    // 변경은 같은 문서 안이라 페이지를 안 떠난다 — markdown 이 링크를
    // `#tasty-nav:` 로 보내는 것과 같은 길이다(실측: type 이
    // `formSubmitted` 가 아니라 `other` 로 오고 문서가 살아 있다).
    //
    // ⚠️ **JS 가 없으면 폼이 그대로 제출된다**(깜빡이되 동작은 한다).
    // 스크립트가 없으면 아무것도 안 되는 화면은 만들지 않는다.
    h.push_str(&format!(
        r##"<form id="f" method="get" action="#">
 <span class="bar">&#x2503;</span><span class="caret">&gt;</span>
 <textarea name="t" id="t" rows="1" autofocus autocomplete="off" placeholder="{ph}">{draft}</textarea>
 <input type="hidden" name="{MARK}" value="{nonce}">
 <button type="submit" id="send" title="{send_hint}">{send}</button>{stop_button}{clear_button}
</form>
<script>
(function(){{
var f=document.getElementById('f'), t=document.getElementById('t');
if(!f||!t)return;
var log=document.getElementById('log');

/* 보낸 말을 **즉시** 대기 칸에 올린다(번호를 단 임시 항목).

   보낼지 쌓을지는 plugin 이 정한다 — 세션이 한가하면 그 항목을 빼고 대화 끝에
   진행 중 턴을 붙이고, 턴이 돌고 있으면 확정 항목으로 남긴다(`send_queue`).
   여기서 판정을 흉내 내지 않는 이유는 둘이 어긋나면 화면의 턴 자리와 plugin 의
   턴 자리가 갈리기 때문이다. plugin 루프가 밀려 판정이 늦어도, 친 말은 바로 보인다. */
function stage(v,n){{
  var q=document.getElementById('queue');
  if(!q)return;
  var d=document.createElement('div');
  d.className='q staged'; d.setAttribute('data-n',n);
  d.textContent=v;   /* textContent — 이스케이프는 브라우저가 */
  q.appendChild(d); q.hidden=false;
}}

/*  **nonce 를 여기서 올린다**(0.1.24).

   서버가 박아 준 값은 문서를 다시 실어야 바뀐다. 그런데 연달아 보내면
   그 사이 재적재가 없어 **같은 값으로 두 번** 간다 — 그리고
   `location.hash` 는 **완전히 같은 값이면 통지를 안 낸다**(실측: 3번
   설정에 2번만 왔다). 즉 같은 말을 두 번 보내면 둘째가 사라진다.

   서버 값을 **시작점**으로만 쓰고, 보낼 때마다 여기서 올린다. */
var n = {nonce};

/* **입력 히스토리** — ↑ 는 이전에 보낸 말, ↓ 는 다음 말(셸과 같다). 목록은 서버가 그릴 때
   이 탭의 보낸 말로 싣는다(`input_history`) — 문서를 다시 실으면 스크립트 상태가 지워지므로
   여기에만 두면 재적재마다 사라진다. 그 사이에 보낸 말은 여기서 덧붙인다.

   커서가 **첫 줄**일 때만 ↑ 를, **마지막 줄**일 때만 ↓ 를 가로챈다 — 여러 줄 글 안에서는
   화살표가 줄을 옮겨야 한다. 끝을 지나 내려오면 쓰던 글(`draft`)로 돌아간다. */
var hist = {hist}, hi = hist.length, draft = '';
function recall(v){{ t.value=v; autosize(); t.selectionStart=t.selectionEnd=v.length; keepView(); }}

/* **메모리 절약**(`view_state`) — 켜져 있으면 가려진 탭의 WebView 가 내려가 입력칸이 사라진다. 쓰던 글을
   300ms 묶어 plugin 에 보내 두면 다시 실은 문서가 그 글로 입력칸을 채운다. 켜고 끄기는 `view.save`
   메시지가 이 값을 바꾼다. 보낼 때 비운 칸은 plugin 이 보내기 표식을 보고 비운다. */
window.__asSaveView = {save};
var vn = 0, vt = null, kept = t.value;
function keepView(){{
  if(!window.__asSaveView)return;
  clearTimeout(vt);
  vt = setTimeout(function(){{
    if(t.value===kept)return;
    kept = t.value; vn = vn + 1;
    location.hash='{VIEW}='+vn+'&d='+encodeURIComponent(kept);
  }}, 300);
}}
t.addEventListener('input',keepView);
t.addEventListener('keydown',function(e){{
  if(e.key!=='ArrowUp'&&e.key!=='ArrowDown')return;
  if(e.shiftKey||e.altKey||e.metaKey||e.ctrlKey)return;
  if(e.isComposing||e.keyCode===229)return;   /* IME 후보 고르기의 화살표다 */
  if(e.key==='ArrowUp'){{
    if(t.value.lastIndexOf('\n',t.selectionStart-1)!==-1||hi===0)return;
    if(hi===hist.length)draft=t.value;
    hi=hi-1; e.preventDefault(); recall(hist[hi]);
  }}else{{
    if(t.value.indexOf('\n',t.selectionEnd)!==-1||hi>=hist.length)return;
    hi=hi+1; e.preventDefault(); recall(hi===hist.length?draft:hist[hi]);
  }}
}});

/* **쉘과 같은 편집 키** — 터미널 서피스는 Home/End(fn+←/→)를 줄 처음·끝으로, PageUp/PageDown
   (fn+↑/↓)을 스크롤로 쓴다. macOS 의 textarea 는 Home/End 로 커서를 옮기지 않고, 이 webview 에서는 그
   키의 기능 문자가 깨진 글자로 들어갔다(실측 2026-10-22). 같은 뜻으로 맞춘다 — Home/End 는 커서가 있는
   줄의 처음·끝(Shift 면 선택을 늘린다, ⌘·Ctrl 이면 글 전체의 처음·끝), ⌘↑/⌘↓ 는 글 전체의 처음·끝
   (Shift 면 선택을 늘린다), PageUp/PageDown 은 대화를 한 화면씩 넘긴다. 나머지 편집 키(⌥+화살표 ·
   ⌘←/→ · ⌥·⌘+⌫ · fn+⌫)는 textarea 가 이미 같은 뜻으로 한다. */
t.addEventListener('keydown',function(e){{
  if(e.isComposing||e.keyCode===229)return;
  var k=e.key;
  if(k==='PageUp'||k==='PageDown'){{
    var lg=document.getElementById('log');
    if(!lg)return;
    e.preventDefault();
    lg.scrollTop+=(k==='PageUp'?-1:1)*Math.max(40,lg.clientHeight-40);
    return;
  }}
  // ⌘↑ · ⌘↓ = 글 전체의 처음 · 끝(macOS 관례 — 노트북에는 Home/End 키가 없다).
  var doc=e.metaKey&&!e.ctrlKey&&(k==='ArrowUp'||k==='ArrowDown');
  if(!doc&&((k!=='Home'&&k!=='End')||e.altKey))return;
  if(doc&&e.altKey)return;
  e.preventDefault();
  var v=t.value, back=t.selectionDirection==='backward';
  var head=back?t.selectionStart:t.selectionEnd, anchor=back?t.selectionEnd:t.selectionStart;
  var pos;
  if(doc)pos=k==='ArrowUp'?0:v.length;
  else if(e.metaKey||e.ctrlKey)pos=k==='Home'?0:v.length;
  else if(k==='Home')pos=v.lastIndexOf('\n',head-1)+1;
  else{{ var i=v.indexOf('\n',head); pos=i===-1?v.length:i; }}
  if(!e.shiftKey)t.setSelectionRange(pos,pos);
  else if(pos<anchor)t.setSelectionRange(pos,anchor,'backward');
  else t.setSelectionRange(anchor,pos,'forward');
}});

/* Enter = send · Shift+Enter = newline (chat/TUI convention).
   A textarea inserts a newline on Enter by default, so submission has to
   be wired explicitly — otherwise the box only ever grows. */
t.addEventListener('keydown',function(e){{
  if(e.key!=='Enter'||e.shiftKey)return;
  if(e.isComposing||e.keyCode===229)return;   /* IME 조합 중은 확정 키다 */
  e.preventDefault();
  if(typeof f.requestSubmit==='function')f.requestSubmit();
  else f.dispatchEvent(new Event('submit',{{cancelable:true}}));
}});
/* **중지** — 단추와 Esc 가 같은 일을 한다. 번호는 보내기(`n`)와 따로 센다: `n` 은
   대기 칸의 임시 항목과 짝을 맞추는 값이라 여기서 올리면 짝이 어긋난다. 같은 hash 는
   통지가 안 나가므로 누를 때마다 올린다.

   `도는 중` 은 **누르는 순간의 문서**로 판정한다 — 진행 중 턴(`.turn.pending`)이나 진행
   표시(`#act`, 이 탭에서 보내지 않은 턴)가 있으면 돈다. 그릴 때의 값으로 정하면 안 된다:
   턴은 문서를 다시 싣지 않고 스크립트로 붙으므로 그 값이 곧 낡는다(실측: 보낸 직후
   단추가 안 보였다). 단추의 보이기도 같은 두 신호를 CSS `:has()` 로 본다.

   Esc 는 턴이 도는 동안에만 먹는다 — 한가할 때 가로채면 IME·다른 용도의 Esc 를
   이유 없이 빼앗는다. IME 조합 중인 Esc 는 조합 취소라 건드리지 않는다. */
var sn = 0;
function running(){{
  var a=document.getElementById('act');
  return !!document.querySelector('.turn.pending') || !!(a && !a.hidden);
}}
/* 멈춘 뒤 입력칸으로 돌아간다 — 단추를 누르면 포커스가 단추로 옮겨, 이어 친 말이
   갈 곳 없이 사라졌다(실측 2026-10-16: 중지 뒤 두 말이 화면에 안 남았다). */
function stop(){{ sn = sn + 1; location.hash='{STOP}='+sn; t.focus(); }}
var sb=document.getElementById('stop');
if(sb)sb.addEventListener('click',function(e){{ e.preventDefault(); stop(); }});
document.addEventListener('keydown',function(e){{
  if(e.key!=='Escape'||!running())return;
  if(e.isComposing||e.keyCode===229)return;
  e.preventDefault();
  stop();
}});
/* grow with content, up to the CSS max-height.
   `#log` 은 남은 높이를 다 차지한다(`flex:1`) — 칸을 `auto` 로 접는 순간 로그가 커져 바닥에 있던
   스크롤이 줄어든 최댓값으로 깎이고, 칸이 다시 커져도 깎인 값이 남아 글자마다 위로 밀렸다.
   그래서 바꾸기 전 바닥까지의 거리를 재 두고 바꾼 뒤 같은 거리로 되돌린다. */
function autosize(){{
  var end = log ? log.scrollHeight - log.scrollTop - log.clientHeight : 0;
  t.style.height='auto';
  t.style.height=t.scrollHeight+'px';
  if(log) log.scrollTop = log.scrollHeight - log.clientHeight - end;
}}
t.addEventListener('input',autosize);
autosize();

f.addEventListener('submit',function(e){{
  var v=t.value;
  if(!v||!v.trim())return e.preventDefault();
  e.preventDefault();
  //  **보낸 뒤 칸을 비운다.** 제출이 페이지를 안 떠나므로 브라우저가
  // 칸을 지워 주지 않는다 — 안 비우면 다음 입력이 앞의 말에 이어붙는다.
  t.value='';
  autosize();   /* 비웠으면 한 줄로 되돌린다 */
  /*  **입력칸을 막지 않는다**(0.1.23). 답을 기다리는 동안에도 다음 말을 칠 수
     있다 — 그 말은 plugin 이 쌓았다가 턴이 끝나면 함께 보낸다(`send_queue`). */
  if(hist[hist.length-1]!==v)hist.push(v);
  hi = hist.length; draft = '';
  clearTimeout(vt); kept = '';
  n = n + 1;
  stage(v, n);
  location.hash='{MARK}='+n+'&t='+encodeURIComponent(v);
}});

/* 이미 쌓인 대화가 있으면 **바닥에서 시작한다** — 매번 문서를 통째로
   다시 싣기 때문에 스크롤이 위로 돌아간다. */
if(log)log.scrollTop=log.scrollHeight;
}})();
</script>"##,
        MARK = FRAG_MARK,
        STOP = STOP_MARK,
        nonce = convo.nonce,
        draft = escape(&convo.view.draft),
        save = convo.view.save,
        VIEW = crate::application::view_state::VIEW_MARK,
        hist = input_history(convo),
        ph = crate::domain::i18n::t("sup.chat.input_placeholder"),
        send = crate::domain::i18n::t("sup.chat.send_button"),
        send_hint = escape(&crate::domain::i18n::t("sup.chat.send_hint")),
        stop_button = stop_button(),
        clear_button = clear_button::button_html(),
    ));
    h.push_str(crate::adapters::driving::messages::script_html());
    h.push_str(&crate::application::prompts::script_html());
    h.push_str(&clear_button::script_html());
    h.push_str(&crate::adapters::driving::md_table::script_html());
    h.push_str(&crate::adapters::driving::links::script_html());
    let skills = convo
        .session_id
        .as_deref()
        .map(crate::adapters::driven::skills::cached)
        .unwrap_or_default();
    h.push_str(&crate::domain::commands::script_html(
        convo.agent_kind.as_deref(),
        &skills,
    ));
    h.push_str(crate::adapters::driving::statusline::style_html());
    h.push_str(crate::domain::turn_time::style_html());
    h.push_str(&crate::adapters::driving::client::script_html(convo, busy));
    h
}

/// 입력 히스토리 — 이 탭에서 보낸 말을 오래된 순으로, 스크립트에 실을 JSON 배열로.
///
/// 이어진 같은 말은 하나로 접고(셸 히스토리와 같다), 최근 [`HISTORY_MAX`] 개만 싣는다. 에이전트가
/// 스스로 이어 간 턴은 보낸 말이 비어 있어 빠진다. 대기 칸에 쌓인 말도 보낸 말이다.
fn input_history(convo: &Convo) -> String {
    let mut out: Vec<&str> = Vec::new();
    let sent = convo
        .turns
        .iter()
        .map(|t| t.sent.as_str())
        .chain(convo.queued.iter().map(String::as_str))
        .filter(|v| !v.trim().is_empty());
    for v in sent {
        if out.last() != Some(&v) {
            out.push(v);
        }
    }
    let from = out.len().saturating_sub(HISTORY_MAX);
    // `<` 를 전부 `\u003c` 로 — 이 값은 `<script>` 안에 그대로 실린다. `</` 만 끊으면
    // `<!--<script` 같은 말이 HTML 의 script 이중 이스케이프 상태를 열어 문서를 깬다.
    // JSON 과 JS 둘 다 같은 글자로 읽는다.
    serde_json::Value::from(&out[from..])
        .to_string()
        .replace('<', r"\u003c")
}

/// 입력 히스토리에 싣는 말의 수 — 문서마다 실리므로 상한을 둔다.
const HISTORY_MAX: usize = 200;

/// 중지 단추 — 늘 문서에 두고 **턴이 도는 동안만 보인다**(CSS `:has()`). 그릴 때 넣고
/// 빼면 스크립트로 붙는 턴을 못 따라간다 — 턴 시작은 문서를 다시 싣지 않는다.
fn stop_button() -> String {
    format!(
        r#"<button type="button" id="stop" class="stop" title="{}">{}</button>"#,
        escape(&crate::domain::i18n::t("sup.chat.stop_hint")),
        escape(&crate::domain::i18n::t("sup.chat.stop_button")),
    )
}

/// 대화창 하단 상태줄 — **`Convo` 가 이미 아는 것만** 싣는다.
///
/// # 왜 조회하지 않나
///
/// 상태(`running`/`stopped`)를 데몬에 물으면 더 정확하지만, 이 surface 는
/// 갱신이 **문서 전체 재적재**라 렌더마다 왕복이 붙는다. 그 비용이 곧
/// 깜빡임이다(0.1.17 이 `push` 하나를 줄인 것과 같은 축).
///
/// 그래서 여기 값은 *"이 탭이 관측한 것"* 이지 데몬의 현재 상태가
/// 아니다 — 세션이 바깥에서 죽어도 이 줄은 모른다. 대시보드가 그 축을
/// 맡는다.
pub fn status_line(convo: &Convo, busy: bool) -> serde_json::Value {
    crate::adapters::driving::statusline::render(convo, busy)
}

#[cfg(test)]
pub use start_form::render_start;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod start_form_tests;

#[cfg(test)]
mod spec_conformance_tests;

#[cfg(test)]
mod reopen_tests;

#[cfg(test)]
mod start_fields_tests;
