// 대화 탭 페이지 앱 — 서버가 실은 상태(window.__AS_STATE)로 대화 로그를 그리고, plugin 이
// webview.post_message 로 보낸 메시지를 store 에 반영해 다시 그린다(Preact + htm).
// 답 글은 textContent 로만 넣고, 표는 그 요소 안에서만 짓는다 — Preact 가 그 요소의 자식은
// 관리하지 않으므로 표 변환이 비교를 깨지 않는다.
(function () {
  'use strict';
  var P = window.preact, Hk = window.preactHooks, htm = window.htm;
  var root = document.getElementById('log');
  var statusRoot = document.getElementById('statusbar');
  var askRoot = document.getElementById('askbar');
  var panelRoot = document.getElementById('subpanelbar');
  if (!P || !Hk || !htm || !root) return;
  var html = htm.bind(P.h);
  var S = window.__AS_STATE || {};
  var L = S.labels || {};
  var log = S.log || { turns: [], older: { archived: 0, items: [] }, sys: [] };
  var subs = S.subs || { row: { hidden: true }, panel: { hidden: true, cards: [] } };
  var status = S.status || { lines: [] };
  var copied = null;
  var ask = S.ask || null, askError = null;
  var nextKey = 1;
  function keyed(t) { t.k = nextKey++; return t; }
  log.turns.forEach(keyed);
  log.older.items.forEach(keyed);

  function p2(n) { return (n < 10 ? '0' : '') + n; }
  function day(at) { var d = new Date(at * 1000); return d.getFullYear() + '-' + p2(d.getMonth() + 1) + '-' + p2(d.getDate()); }
  function clock(at) { var d = new Date(at * 1000); return '[' + p2(d.getHours()) + ':' + p2(d.getMinutes()) + ':' + p2(d.getSeconds()) + '] '; }
  function now() { return Math.floor(Date.now() / 1000); }

  // 답 · 실패 글. 글이 바뀔 때만 다시 넣고 표를 짓는다.
  function Answer(props) {
    var ref = Hk.useRef(null);
    Hk.useLayoutEffect(function () {
      var g = ref.current;
      if (!g) return;
      g.textContent = props.text;
      g.removeAttribute('data-tbl');
      if (window.tastyTables) window.tastyTables(g);
      g.removeAttribute('data-lnk');
      if (window.tastyLinks) window.tastyLinks(g);
    }, [props.text, props.cls]);
    return html`<div class=${props.cls} ref=${ref}></div>`;
  }

  function Sent(props) {
    var t = props.t, auto = t.sent === '';
    return html`<div class=${'sent' + (auto ? ' auto' : '')}>${t.at != null ? html`<span class="ts">${clock(t.at)}</span>` : null}${auto ? L.auto : t.sent}</div>`;
  }

  function Body(props) {
    var t = props.t;
    if (t.error != null) return html`<${Answer} cls="err" text=${t.error} />`;
    if (t.stopped) return html`<div class="got stopped">${L.stopped}</div>`;
    if (t.pending) {
      return html`<div class="got"><span class="spin"></span><span class="think">${t.overdue ? (t.overdueText || L.overdue) : L.thinking}</span></div>`;
    }
    return html`<${Answer} cls="got" text=${t.got} />`;
  }

  function Turn(props) {
    var t = props.t;
    return html`<div class=${'turn' + (t.pending ? ' pending' : '')} data-at=${t.at != null ? String(t.at) : undefined}><${Sent} t=${t} /><${Body} t=${t} /></div>`;
  }

  function OlderTurn(props) {
    var t = props.t;
    return html`<div class="oturn" data-at=${t.at != null ? String(t.at) : undefined}><${Sent} t=${t} /><${Answer} cls=${t.error != null ? 'err' : 'got'} text=${t.error != null ? t.error : t.got} /></div>`;
  }

  function SysLine(props) {
    var n = props.n;
    return html`<div class=${'sys' + n.cls} id=${'sys' + n.id}>${n.text}<span class="st">${n.st}</span></div>`;
  }

  // 시각을 아는 턴 위에 날짜가 바뀔 때마다 날짜 줄을 둔다.
  function withDays(list, Item) {
    var out = [], last = '';
    list.forEach(function (t) {
      if (t.at != null) {
        var d = day(t.at);
        if (d !== last) { out.push(html`<div class="day" key=${'d' + t.k}>[${d}]</div>`); last = d; }
      }
      out.push(html`<${Item} key=${t.k} t=${t} />`);
    });
    return out;
  }

  function Older() {
    var o = log.older, remaining = Math.max(0, o.archived - o.items.length);
    return html`<div id="older" class="older" hidden=${o.archived === 0}><a class="more" href=${'#' + L.olderMark + '=' + (o.items.length + 1)} hidden=${remaining === 0}>${L.older} (<span class="n">${remaining}</span>)</a>${withDays(o.items, OlderTurn)}</div>`;
  }

  // 에이전트 내역 — 줄은 로그 안, 패널은 문서 오른쪽. 카드 · 단계는 plugin 이 준 바뀌지 않는 k 를
  // key 로 써서 다시 그려도 같은 details 가 남는다 — 펼침과 스크롤이 그대로다.
  function SubsRow() {
    var r = subs.row;
    return html`<div id="subs" class="older" hidden=${!!r.hidden}><a class="more" href=${'#' + r.mark + '=1'}>${r.label}</a></div>`;
  }
  function Step(props) {
    var s = props.s;
    return html`<div class=${'ss ' + s.cls}>${s.at != null ? html`<span class="ts">${clock(s.at)}</span>` : null}<span class="sok">${s.mark}</span><b>${s.tool}</b><span class="ssm" title=${s.summary}>${s.summary}</span></div>`;
  }
  function Steps(props) {
    var c = props.c, p = subs.panel;
    if (!c.steps.length) return html`<div class="sab">${p.stepsLabel}</div><div class="sax none">${p.noSteps}</div>`;
    return html`<div class="sab">${p.stepsLabel}</div><div class="sst">${c.omitted != null ? html`<div class="sso">${c.omitted}</div>` : null}${c.steps.map(function (s) { return html`<${Step} key=${s.k} s=${s} />`; })}</div>`;
  }
  function SubCard(props) {
    var c = props.c, p = subs.panel;
    var talk = c.prompt != null
      ? html`<div class="sab">${p.promptLabel}</div><div class="sax">${c.prompt}</div><div class="sab">${p.answerLabel}</div>${c.answer ? html`<div class="sax">${c.answer}</div>` : html`<div class="sax none">${p.noAnswer}</div>`}`
      : null;
    return html`<details class=${'sa ' + c.cls} data-k=${c.k}><summary><span class="sat"><span class="sas">${c.mark}</span><span class="sad" title=${c.desc}>${c.desc}</span></span><span class="sam">${c.meta}</span></summary>${talk}${c.steps != null ? html`<${Steps} c=${c} />` : null}</details>`;
  }
  function SubPanel() {
    var p = subs.panel;
    return html`<aside id="subpanel" hidden=${!!p.hidden}><div class="sph"><b>${p.title}</b><span class="cnt">${p.count}</span><a href=${'#' + p.reloadMark + '=1'} title=${p.reloadTitle}>↻</a><a href=${'#' + p.closeMark + '=1'} title=${p.closeTitle}>✕</a></div><div class="spl">${p.empty != null ? html`<div class="spe">${p.empty}</div>` : null}${(p.cards || []).map(function (c) { return html`<${SubCard} key=${c.k} c=${c} />`; })}</div></aside>`;
  }
  function drawPanel() { if (panelRoot) P.render(html`<${SubPanel} />`, panelRoot); }

  function Log() {
    var out = [html`<${Older} key="older" />`, html`<${SubsRow} key="subs" />`];
    if (!log.turns.length) out.push(html`<p class="empty" key="empty">${L.empty}</p>`);
    var last = '';
    for (var i = 0; i <= log.turns.length; i++) {
      log.sys.forEach(function (n) { if (n.at === i) out.push(html`<${SysLine} key=${'s' + n.id} n=${n} />`); });
      var t = log.turns[i];
      if (!t) continue;
      if (t.at != null) {
        var d = day(t.at);
        if (d !== last) { out.push(html`<div class="day" key=${'d' + t.k}>[${d}]</div>`); last = d; }
      }
      out.push(html`<${Turn} key=${t.k} t=${t} />`);
    }
    return out;
  }

  // 상태줄 — 서버가 계산한 줄 · 토큰을 그대로 그린다.
  function resetTime(ts) {
    var d = new Date(ts * 1000);
    return (d - Date.now() < 864e5)
      ? d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
      : p2(d.getMonth() + 1) + '-' + p2(d.getDate()) + ' ' + p2(d.getHours()) + ':' + p2(d.getMinutes());
  }
  function copyLink(mark) {
    return function (e) { e.preventDefault(); location.hash = mark + '=' + Date.now(); };
  }
  // 유휴 시계 — 만료 예정 시각을 현지 시각으로 그린다. 색(60초 전 · 지난 뒤)만 시간이 흐르며 바뀐다(idle_clock.rs).
  function hms(at) { var d = new Date(at * 1000); return p2(d.getHours()) + ':' + p2(d.getMinutes()) + ':' + p2(d.getSeconds()); }
  function Clock(props) {
    var k = props.k, s = k.ts - now();
    var cls = s <= 0 ? k.over : (s <= 60 ? 'wait' : undefined);
    return html`<span title=${k.title}><span class="k">${k.label}:</span><b class=${cls}><time data-until=${String(k.ts)}>${hms(k.ts)}${s <= 0 ? '(' + k.done + ')' : ''}</time></b></span>`;
  }
  function hasClock() {
    return status.lines.some(function (l) { return l.some(function (k) { return k.t === 'clock'; }); });
  }
  setInterval(function () { if (hasClock()) drawStatus(); }, 1000);
  function Tok(props) {
    var k = props.k;
    return html`<span title=${k.title || undefined}>${k.label ? html`<span class="k">${k.label}:</span>` : null}<b class=${k.cls || undefined}>${k.val}</b></span>`;
  }
  function Token(props) {
    var k = props.k, note = copied && copied.cls === k.t ? html`<span class="ok">${' ' + copied.text}</span>` : null;
    switch (k.t) {
      case 'tok': return html`<${Tok} k=${k} />`;
      case 'k': return html`<span class="k">${k.text}</span>`;
      case 'perm': return html`<span class="k">⏵⏵</span> <${Tok} k=${k.tok} />`;
      case 'cwd': return html`<a class="cwd" href="#" title=${k.title} onClick=${copyLink(k.mark)}><b>${k.val}</b></a>${note}`;
      case 'sid': return html`<a class="sid" href="#" title=${k.title} onClick=${copyLink(k.mark)}><span class="k">${k.label}:</span><b>${k.val}</b></a>${note}`;
      case 'sess': return html`${k.pre || ''}<a class="sess" href="#" title=${k.title} onClick=${copyLink(k.mark)}>${k.label ? html`<span class="k">${k.label}:</span>` : null}<b>${k.val}</b></a>${k.post || ''}${note}`;
      case 'limit': return html`<span title=${k.title}><span class="k">${k.label}:</span><b class=${k.cls}>${k.pct}%</b>(<time data-ts=${String(k.ts)}>${resetTime(k.ts)}</time>)</span>`;
      case 'clock': return html`<${Clock} k=${k} />`;
      case 'plain': return html`<span class=${k.cls}>${k.text}</span>`;
    }
    return null;
  }
  function StatusBar() {
    var lines = status.lines.map(function (l, i) {
      var parts = [];
      l.forEach(function (k, j) { if (j) parts.push('  '); parts.push(html`<${Token} k=${k} />`); });
      return html`<div class="ln" key=${i}>${parts}</div>`;
    });
    return html`<div class=${'status' + (status.folded ? ' folded' : '')}><a class="fold" href=${'#' + status.foldMark + '=1'} title=${status.foldTitle}>${status.arrow}</a><div class="lines">${lines}</div></div>`;
  }
  function drawStatus() { if (statusRoot) P.render(html`<${StatusBar} />`, statusRoot); }

  // 질문 카드 — 선택은 일반 input 에 두고, 보내기는 문서의 tastyAsk 가 그 선택을 읽어 보낸다.
  function respond(allow) {
    return function (e) { e.preventDefault(); if (window.tastyAsk) window.tastyAsk(ask.id, allow); };
  }
  function AskCard() {
    if (!ask) return html`<div id="ask" hidden></div>`;
    var body = ask.questions
      ? ask.questions.map(function (q, qi) {
        return html`<div class="q" data-q=${q.text} key=${qi}><b>${q.text}</b>${q.options.map(function (o, oi) {
          return html`<label key=${oi}><input type=${q.multi ? 'checkbox' : 'radio'} name=${'q' + qi} value=${o.label} />${' ' + o.label + ' '}<span>${o.desc}</span></label>`;
        })}</div>`;
      })
      : html`<pre>${ask.preview}</pre>`;
    return html`<div id="ask"><div class="t">${ask.title}</div>${body}<div class="acts"><a href="#" onClick=${respond(true)}>${ask.yes}</a><a class="no" href="#" onClick=${respond(false)}>${ask.no}</a></div>${askError != null ? html`<div class="err">${askError}</div>` : null}</div>`;
  }
  function drawAsk() { if (askRoot) P.render(html`<${AskCard} />`, askRoot); }

  function nearEnd() { return root.scrollHeight - root.scrollTop - root.clientHeight < 48; }
  function draw(scroll) {
    P.render(html`<${Log} />`, root);
    if (scroll) root.scrollTop = root.scrollHeight;
  }

  // 메시지를 store 에 반영한다. 다 처리했으면 true — false 면 페이지의 다른 처리기가 이어 받는다.
  function apply(m) {
    var t;
    switch (m.type) {
      case 'ask.set':
        // 카드가 나타나면 로그가 짧아진다 — 끝을 보고 있었으면 끝을 그대로 보인다.
        var keep = nearEnd();
        ask = m.ask; askError = null;
        drawAsk();
        if (keep) root.scrollTop = root.scrollHeight;
        return true;
      case 'ask.error':
        if (ask) { askError = m.text; drawAsk(); }
        return true;
      case 'status.set':
        status = m.status;
        drawStatus();
        return true;
      case 'status.copied':
        copied = { cls: m.cls, text: m.text };
        drawStatus();
        setTimeout(function () { copied = null; drawStatus(); }, 1500);
        return true;
      case 'turn.settle':
        t = log.turns[m.at];
        if (!t) return true;
        // 맨 아래를 보고 있었으면 답이 길어져도 끝을 따라간다 — 위를 읽던 사람은 그대로 둔다.
        var follow = nearEnd();
        t.pending = false; t.overdue = false;
        if (m.cls === 'err') { t.error = m.text; }
        else if (m.cls.indexOf('stopped') >= 0) { t.stopped = true; }
        else { t.got = m.text; t.error = null; }
        draw(follow);
        return true;
      case 'turn.overdue':
        t = log.turns[m.at];
        if (t) { t.overdue = true; t.overdueText = m.text; draw(false); }
        return true;
      case 'turn.auto':
        var near = nearEnd();
        log.turns.push(keyed(m.cls === 'err'
          ? { sent: '', got: '', error: m.text, at: now() }
          : { sent: '', got: m.text, error: null, at: now() }));
        draw(near);
        return true;
      case 'turns.trim':
        log.turns.splice(0, m.cut);
        log.sys = log.sys.filter(function (n) { return n.at >= m.cut; });
        log.sys.forEach(function (n) { n.at -= m.cut; });
        log.older.archived = m.archived;
        draw(false);
        return true;
      case 'sys.line':
        var found = false;
        log.sys.forEach(function (n, i) { if (n.id === m.note.id) { log.sys[i] = m.note; found = true; } });
        if (!found) log.sys.push(m.note);
        draw(!found);
        return true;
      case 'queue.settle':
        if (m.sent != null) {
          log.turns.push(keyed({ sent: m.sent, got: '', error: null, pending: true, at: now() }));
          draw(true);
        }
        return false;
      case 'subs.set':
        subs = m.subs;
        draw(false);
        drawPanel();
        return true;
    }
    return false;
  }

  window.__asApp = { apply: apply, state: log, status: function () { return status; }, subs: function () { return subs; } };
  drawStatus();
  drawAsk();
  drawPanel();
  draw(true);
})();
