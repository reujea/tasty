// 마크다운 편집기 웹뷰 스크립트(plugin 이 생성 문서 끝에 싣는다 — 신뢰 스크립트).
// 웹뷰 → plugin: location.hash = 'tasty-nav:mdedit:<seq>:<op>:<enc JSON>' (docs/surfaces/mdedit/index.md#웹뷰-입력은-nav-fragment-로-보낸다).
// plugin → 웹뷰: webview.post_message 의 tasty:message — {type:"apply",u:{...}} · {type:"save"}.
(function () {
  'use strict';
  var I = window.__MDEDIT_INIT || {};
  var CHUNK = I.chunk || 16384;
  var ACK_TIMEOUT_MS = 1500;
  var FLUSH_MS = 300;
  var seq = 0;
  var queue = [];
  var inflight = 0;
  var inflightTimer = null;
  var stateKey = 'mdedit:' + (I.file || '');

  function nav(body) {
    seq += 1;
    inflight = seq;
    clearTimeout(inflightTimer);
    // ack 가 안 오면(유실) 다음으로 넘어간다 — plugin 은 번호 틈을 보고 재동기를 요청한다.
    inflightTimer = setTimeout(function () { inflight = 0; pump(); }, ACK_TIMEOUT_MS);
    location.hash = 'tasty-nav:mdedit:' + seq + ':' + body;
  }

  function pump() {
    if (inflight || !queue.length) return;
    nav(queue.shift());
  }

  // op 와 payload(객체) 하나를 보낸다. 인코딩 뒤 CHUNK 를 넘으면 조각으로 나눈다.
  function send(op, obj) {
    var json = obj == null ? '' : JSON.stringify(obj);
    var body = op + ':' + encodeURIComponent(json);
    if (body.length <= CHUNK) {
      queue.push(body);
    } else {
      var raw = op + ':' + json;
      var step = Math.max(256, Math.floor(CHUNK / 4));
      var parts = [];
      for (var i = 0; i < raw.length; i += step) {
        var end = Math.min(raw.length, i + step);
        // 서로게이트 쌍을 가르지 않는다.
        var c = raw.charCodeAt(end - 1);
        if (end < raw.length && c >= 0xd800 && c <= 0xdbff) end -= 1;
        parts.push(raw.slice(i, end));
        i = end - step;
      }
      for (var k = 0; k < parts.length; k++) {
        queue.push('chunk:' + encodeURIComponent(JSON.stringify({ i: k, n: parts.length, d: parts[k] })));
      }
    }
    pump();
  }

  var ta = document.getElementById('mdedit-ta');
  var gutter = document.getElementById('mdedit-gutter');
  var source = I.mode === 'source';
  var synced = source ? (I.text || '') : '';
  var marks = {};
  var flushTimer = null;
  var lineCount = -1;

  document.body.classList.add(source ? 'mdedit-source' : 'mdedit-preview');

  function setMarks(list) {
    marks = {};
    (list || []).forEach(function (m) { marks[m[0]] = m[1]; });
    lineCount = -1;
    renderGutter();
  }

  function renderGutter() {
    if (!source || !gutter) return;
    var n = ta.value.split('\n').length;
    if (n === lineCount) return;
    lineCount = n;
    var html = [];
    for (var i = 1; i <= n; i++) {
      var m = marks[i];
      html.push('<div' + (m ? ' class="g-' + m + '"' : '') + '>' + i + '</div>');
    }
    gutter.innerHTML = html.join('');
    gutter.scrollTop = ta.scrollTop;
  }

  // 마지막으로 plugin 과 맞춘 텍스트 대비 바뀐 한 덩어리.
  function splice(from, to) {
    var a = 0;
    var n = Math.min(from.length, to.length);
    while (a < n && from.charCodeAt(a) === to.charCodeAt(a)) a++;
    if (a > 0) {
      var h = from.charCodeAt(a - 1);
      if (h >= 0xd800 && h <= 0xdbff) a--;
    }
    var b = 0;
    while (b < n - a && from.charCodeAt(from.length - 1 - b) === to.charCodeAt(to.length - 1 - b)) b++;
    if (b > 0) {
      var l = from.charCodeAt(from.length - b);
      if (l >= 0xdc00 && l <= 0xdfff) b--;
    }
    return { s: a, e: from.length - b, t: to.slice(a, to.length - b) };
  }

  function flush() {
    clearTimeout(flushTimer);
    if (!source) return;
    var cur = ta.value;
    if (cur === synced) return;
    var sp = splice(synced, cur);
    sp.len = cur.length;
    send('edit', sp);
    synced = cur;
  }

  function saveState() {
    if (!source) return;
    try {
      sessionStorage.setItem(stateKey, JSON.stringify({ st: ta.scrollTop, ss: ta.selectionStart, se: ta.selectionEnd }));
    } catch (e) {}
  }

  function restoreState() {
    try {
      var s = JSON.parse(sessionStorage.getItem(stateKey) || 'null');
      if (!s) return;
      ta.setSelectionRange(Math.min(s.ss, ta.value.length), Math.min(s.se, ta.value.length));
      ta.scrollTop = s.st;
    } catch (e) {}
  }

  function setReadonly(ro) {
    if (ta) ta.readOnly = !!ro;
    document.querySelectorAll('#tasty-md-body input[type=checkbox]').forEach(function (c) { c.disabled = !!ro; });
  }

  // plugin 이 바꾼 텍스트를 커서·선택·스크롤을 지키며 넣는다. 아직 안 보낸 사용자 입력은
  // plugin 변경과 겹치지 않으면 그 위에 다시 얹는다.
  function replaceText(text) {
    var cur = ta.value;
    var local = cur === synced ? null : splice(synced, cur);
    var server = splice(synced, text);
    var next = text;
    if (local) {
      var delta = text.length - synced.length;
      if (local.e <= server.s) {
        next = text.slice(0, local.s) + local.t + text.slice(local.e);
      } else if (local.s >= server.e) {
        next = text.slice(0, local.s + delta) + local.t + text.slice(local.e + delta);
      }
    }
    var d = splice(cur, next);
    var shift = function (p) {
      if (p <= d.s) return p;
      if (p >= d.e) return p + (d.t.length - (d.e - d.s));
      return d.s + d.t.length;
    };
    var ss = shift(ta.selectionStart), se = shift(ta.selectionEnd), st = ta.scrollTop;
    ta.value = next;
    ta.setSelectionRange(ss, se);
    ta.scrollTop = st;
    synced = text;
    lineCount = -1;
    renderGutter();
    if (next !== text) flushTimer = setTimeout(flush, FLUSH_MS);
  }

  window.__mdedit = {
    apply: function (u) {
      if (u.ack && u.ack === inflight) {
        inflight = 0;
        clearTimeout(inflightTimer);
        pump();
      }
      if (u.chrome != null) {
        var c = document.getElementById('mdedit-chrome');
        if (c) c.innerHTML = u.chrome;
        if (window.__mdeFmt) window.__mdeFmt();
      }
      if (u.text != null && source) replaceText(u.text);
      if (u.gutter) setMarks(u.gutter);
      if (u.readonly != null) setReadonly(u.readonly);
      if (u.resend && source) {
        synced = ta.value;
        send('full', { t: ta.value });
      }
    },
    // 저장 단축키(host 키바인딩)가 부른다. 미리보기에서 열린 편집칸이 있으면 먼저 적용한다.
    saveNow: function () { flush(); if (commitInline) commitInline(); send('save', null); }
  };
  window.addEventListener('tasty:message', function (e) {
    var m = e.detail;
    if (!m) return;
    if (m.type === 'apply' && m.u) window.__mdedit.apply(m.u);
    else if (m.type === 'save') window.__mdedit.saveNow();
  });
  var commitInline = null;
  var fold = null;

  document.addEventListener('click', function (e) {
    var t = e.target;
    if (!t || !t.closest) return;
    var b = t.closest('[data-act]');
    if (!b || b.disabled) return;
    e.preventDefault();
    flush();
    // 미리보기 편집칸이 열려 있으면 그 글을 먼저 보낸다 — 저장·모드 전환이 그 뒤에 처리된다.
    if (commitInline) commitInline();
    saveState();
    var act = b.getAttribute('data-act');
    if (act === 'save') send('save', null);
    else send('act', { a: act, x: b.getAttribute('data-arg') || '' });
  });

  if (source && ta) {
    ta.value = synced;
    restoreState();
    setMarks(I.gutter);
    ta.addEventListener('input', function () {
      clearTimeout(flushTimer);
      flushTimer = setTimeout(flush, FLUSH_MS);
      renderGutter();
    });
    ta.addEventListener('scroll', function () {
      if (gutter) gutter.scrollTop = ta.scrollTop;
      saveState();
    });
    ta.addEventListener('select', saveState);
    ta.addEventListener('keyup', saveState);
    ta.addEventListener('blur', flush);
    // 탭 키는 들여쓰기로 쓴다(포커스 이동 대신).
    ta.addEventListener('keydown', function (e) {
      if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && !e.altKey && !ta.readOnly) {
        e.preventDefault();
        var s = ta.selectionStart, en = ta.selectionEnd;
        ta.setRangeText('\t', s, en, 'end');
        ta.dispatchEvent(new Event('input'));
      }
    });
  } else {
    var boxes = document.querySelectorAll('#tasty-md-body input[type=checkbox]');
    boxes.forEach(function (c, i) {
      c.disabled = false;
      c.addEventListener('click', function (e) {
        e.preventDefault();
        if (c.disabled) return;
        send('task', { i: i, n: boxes.length });
      });
    });
    fold = initFold();
    initInline();
  }

  // ---- 접기 (docs/surfaces/mdedit/index.md#접기는-보기-상태) ----
  // 본문 바로 아래 제목은 다음 같은·상위 수준 제목까지를, 하위 목록을 가진 항목은 그 하위 목록을 접는다.
  // 보기 상태일 뿐이다 — 원문·버퍼에 닿지 않고, 접은 자리는 이 탭의 sessionStorage 에 남아 다시 그려도
  // 유지된다. 화살표는 CSS 가 그리므로(::before) 토글은 글자가 없다 — 블록 짝짓기가 그대로다.
  function initFold() {
    var body = document.getElementById('tasty-md-body');
    if (!body) return null;
    var foldKey = stateKey + ':fold';
    var LEVEL = /^H([1-6])$/;
    var closed = {}, seen = {};
    try { (JSON.parse(sessionStorage.getItem(foldKey) || '[]') || []).forEach(function (k) { closed[k] = 1; }); } catch (x) {}
    // 같은 글의 제목이 여럿이면 몇 번째인지로 가른다.
    function keyOf(kind, text) {
      var k = kind + ':' + text.replace(/\s+/g, ' ').trim();
      seen[k] = (seen[k] || 0) + 1;
      return k + '#' + seen[k];
    }
    function ownText(li) {
      var t = '';
      for (var n = li.firstChild; n; n = n.nextSibling) {
        if (n.nodeType === 1 && (n.tagName === 'UL' || n.tagName === 'OL')) break;
        t += n.textContent;
      }
      return t;
    }
    function addToggle(el, key) {
      var t = document.createElement('span');
      t.className = 'mde-fold';
      t.setAttribute('contenteditable', 'false');
      t.setAttribute('aria-hidden', 'true');
      el.insertBefore(t, el.firstChild);
      el.setAttribute('data-fold', key);
      el.classList.add('mde-foldable');
      if (closed[key]) el.classList.add('mde-folded');
    }
    Array.prototype.forEach.call(body.children, function (c) {
      var m = LEVEL.exec(c.tagName);
      if (m) addToggle(c, keyOf('h' + m[1], c.textContent));
    });
    Array.prototype.forEach.call(body.querySelectorAll('li'), function (li) {
      var sub = Array.prototype.some.call(li.children, function (c) { return c.tagName === 'UL' || c.tagName === 'OL'; });
      if (sub) addToggle(li, keyOf('li', ownText(li)));
    });
    function apply() {
      var under = 0;
      Array.prototype.forEach.call(body.children, function (c) {
        var m = LEVEL.exec(c.tagName), lv = m ? +m[1] : 0;
        if (lv && under && lv <= under) under = 0;
        c.classList.toggle('mde-fold-hidden', !!under && !c.classList.contains('mde-addline'));
        if (lv && !under && c.classList.contains('mde-folded')) under = lv;
      });
    }
    function save() {
      var keys = Array.prototype.map.call(body.querySelectorAll('.mde-folded'), function (e) { return e.getAttribute('data-fold'); });
      try { sessionStorage.setItem(foldKey, JSON.stringify(keys)); } catch (x) {}
    }
    // el 을 가린 접기를 푼다 — 그 블록을 편집기로 열거나 목차·링크로 찾아갈 때.
    function reveal(el) {
      if (!el || !body.contains(el)) return;
      var opened = false;
      for (var li = el.parentElement && el.parentElement.closest('li.mde-folded'); li; li = li.parentElement && li.parentElement.closest('li.mde-folded')) {
        li.classList.remove('mde-folded');
        opened = true;
      }
      var top = el;
      while (top.parentElement && top.parentElement !== body) top = top.parentElement;
      if (top.classList.contains('mde-fold-hidden')) {
        var tm = LEVEL.exec(top.tagName), min = tm ? +tm[1] : 7;
        for (var p = top.previousElementSibling; p && min > 1; p = p.previousElementSibling) {
          var pm = LEVEL.exec(p.tagName);
          if (!pm || +pm[1] >= min) continue;
          min = +pm[1];
          if (p.classList.contains('mde-folded')) { p.classList.remove('mde-folded'); opened = true; }
        }
      }
      if (opened) { apply(); save(); }
    }
    // 붙잡는 단계에서 처리한다 — 토글은 편집 가능한 블록 안에 있으므로 편집기 열기·포커스 이동보다 먼저.
    body.addEventListener('mousedown', function (ev) {
      if (ev.target.closest && ev.target.closest('.mde-fold')) ev.preventDefault();
    }, true);
    body.addEventListener('click', function (ev) {
      var t = ev.target.closest && ev.target.closest('.mde-fold');
      if (!t) return;
      ev.preventDefault();
      ev.stopPropagation();
      t.parentNode.classList.toggle('mde-folded');
      apply();
      save();
    }, true);
    document.addEventListener('click', function (ev) {
      var a = ev.target.closest && ev.target.closest('a[href^="#"]');
      var h = a && a.getAttribute('href');
      if (!h || h.indexOf('#tasty-') === 0) return;
      var id = h.slice(1);
      try { id = decodeURIComponent(id); } catch (x) {}
      reveal(document.getElementById(id));
    }, true);
    apply();
    return { reveal: reveal };
  }

  // ---- 미리보기 직접 편집 (docs/surfaces/mdedit/index.md#미리보기-편집-단위는-블록-원문-조각) ----
  // plugin 이 원문에서 뽑은 편집 단위 [종류, 원문 조각, 렌더 글자, 플래그] 를 같은 맥락의 요소와
  // 순서대로 짝짓고, 글자가 같을 때만 연다. 편집은 그 블록의 원문 조각만 고친다 — 렌더 결과를
  // 거꾸로 markdown 으로 만들지 않으므로 손대지 않은 바이트는 그대로다.
  function initInline() {
    if (!I.blocks) return;
    var B = I.blocks;
    var T = I.i18n || {};
    var body = document.getElementById('tasty-md-body');
    if (!body) return;
    var F_EDIT = 1, F_MULTI = 2, F_AFTER = 4, F_HEAD = 16, F_EMPTY_ITEM = 32;
    var LOOKAHEAD = 8;
    var draftKey = stateKey + ':draft';
    var editing = null;
    var menu = null;
    var els = [];

    function norm(s) { return (s || '').replace(/\s+/g, ' ').trim(); }

    var BLOCKISH = /^(UL|OL|PRE|BLOCKQUOTE|TABLE|DIV|DETAILS|FIGURE|H[1-6]|HR|DL)$/;
    // 항목 자신의 첫 줄 글자 — 하위 목록·블록 앞까지. 느슨한 목록이면 첫 문단.
    function ownNodes(li) {
      var out = [];
      for (var n = li.firstChild; n; n = n.nextSibling) {
        if (n.nodeType === 1) {
          if (BLOCKISH.test(n.tagName)) break;
          if (n.tagName === 'P') { if (!out.some(function (x) { return norm(x.textContent); })) out = [n]; break; }
          if (n.tagName === 'INPUT' || n.classList.contains('mde-fold')) continue;
        }
        out.push(n);
      }
      return out;
    }
    function textOf(kind, el) {
      if (kind === 'li') return ownNodes(el).map(function (n) { return n.textContent; }).join('');
      return el.textContent;
    }
    function candidates(kind) {
      var list;
      if (kind === 'p') {
        list = Array.prototype.filter.call(body.children, function (e) { return e.tagName === 'P'; });
      } else if (kind === 'h') {
        list = Array.prototype.filter.call(body.querySelectorAll('h1,h2,h3,h4,h5,h6'), function (e) {
          return !e.closest('blockquote,li,td,th,details,.footnote-definition');
        });
      } else if (kind === 'li') {
        list = Array.prototype.filter.call(body.querySelectorAll('li'), function (e) {
          return !e.closest('blockquote,details,.footnote-definition');
        });
      } else {
        list = Array.prototype.filter.call(body.querySelectorAll('td,th'), function (e) {
          return !e.closest('blockquote,li,details,.footnote-definition');
        });
      }
      return list;
    }

    // 종류마다 원문 순서와 요소 순서를 맞춘다. 요소 쪽에 끼어든 것(렌더러가 만든 것)은 건너뛰고,
    // 글자가 다른 블록은 짝을 안 지운다 — 그 블록은 미리보기에서 열리지 않는다.
    ['p', 'h', 'li', 'td'].forEach(function (kind) {
      var dom = candidates(kind).map(function (e) { return [e, norm(textOf(kind, e))]; });
      var j = 0;
      B.forEach(function (b, id) {
        if (b[0] !== kind) return;
        var want = norm(b[2]);
        for (var k = j; k < dom.length && k < j + LOOKAHEAD; k++) {
          if (dom[k][1] === want) {
            els[id] = dom[k][0];
            dom[k][0].setAttribute('data-mde', String(id));
            if (b[3] & F_EDIT) dom[k][0].classList.add('mde-editable');
            j = k + 1;
            return;
          }
        }
      });
    });


    // ---- 블록 손잡이 단위 (docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다) ----
    // 단위 = 최상위 블록 · 목록 항목(하위 목록째). 문단 · 제목 · 항목은 위에서 글자로 짝지은 요소를
    // 그대로 쓰고, 목록은 첫 항목의 부모로, 나머지 최상위 블록은 그 사이에서 태그 순서로 짝짓는다.
    var UD = I.units || {};
    var U = UD.u || [], UL = UD.l || [], UT = UD.t || [];
    var unitEl = [], listEl = [];
    U.forEach(function (x, u) { if (x[4] >= 0 && els[x[4]]) unitEl[u] = els[x[4]]; });
    UL.forEach(function (l, k) {
      for (var u = 0; u < U.length; u++) {
        if (U[u][1] === k && unitEl[u] && /^(UL|OL)$/.test(unitEl[u].parentNode.tagName)) { listEl[k] = unitEl[u].parentNode; return; }
      }
    });
    (function () {
      var GROUP = { quote: /^(BLOCKQUOTE|DETAILS)$/, code: /^PRE$/, table: /^TABLE$/, hr: /^HR$/, dl: /^DL$/, p: /^(P|FIGURE)$/ };
      var kids = Array.prototype.filter.call(body.children, function (c) { return !/(^| )mde-/.test(c.className || ''); });
      var pos = 0;
      UT.forEach(function (t) {
        var el = t[0] === 1 ? listEl[t[1]] : unitEl[t[1]];
        if (el) { var i = kids.indexOf(el); if (i >= 0) pos = i + 1; return; }
        if (t[0] === 1) return;
        var kind = U[t[1]][0];
        var g = GROUP[kind] || (/^h[1-6]$/.test(kind) ? new RegExp('^' + kind.toUpperCase() + '$') : null);
        if (!g) return;
        for (var j = pos; j < kids.length && j < pos + 3; j++) {
          if (g.test(kids[j].tagName)) { unitEl[t[1]] = kids[j]; pos = j + 1; return; }
        }
      });
    })();
    unitEl.forEach(function (el, u) { if (el) el.setAttribute('data-mu', String(u)); });

    var add = document.createElement('div');
    add.className = 'mde-addline';
    add.textContent = '+ ' + (T.add_line || '');
    body.appendChild(add);

    function saveDraft() {
      if (!editing) return;
      try {
        sessionStorage.setItem(draftKey, JSON.stringify({
          id: editing.id, o: editing.orig, v: currentValue(editing), n: editing.isNew,
          ao: editing.isNew && editing.id != null ? B[editing.id][1] : null,
          u: editing.unit, uh: editing.unit != null ? U[editing.unit][3] : null
        }));
      } catch (e) {}
    }
    function clearDraft() { try { sessionStorage.removeItem(draftKey); } catch (e) {} }

    function autosize(ta) { ta.style.height = 'auto'; ta.style.height = ta.scrollHeight + 'px'; }

    function hintFor(e) {
      var multi = e.multi || (!e.isNew && (B[e.id][3] & F_MULTI));
      return multi ? T.hint_multi : T.hint;
    }

    function buildBox(e) {
      var box = document.createElement(e.kind === 'li' || e.kind === 'td' ? 'span' : 'div');
      box.className = 'mde-inline';
      var ta = document.createElement('textarea');
      ta.className = 'mde-inline-ta';
      ta.rows = 1;
      ta.spellcheck = false;
      ta.value = e.orig;
      box.appendChild(ta);
      var hint = document.createElement('span');
      hint.className = 'mde-inline-hint';
      box.appendChild(hint);
      if (e.kind === 'td') {
        var head = B[e.id][3] & F_HEAD;
        [['row_above', false], ['row_below', true]].forEach(function (r) {
          if (head && !r[1]) return;
          var btn = document.createElement('button');
          btn.type = 'button';
          btn.className = 'mde-inline-btn';
          btn.textContent = T[r[0]] || r[0];
          btn.addEventListener('mousedown', function (ev) { ev.preventDefault(); });
          btn.addEventListener('click', function (ev) {
            ev.preventDefault();
            ev.stopPropagation();
            if (!editing) return;
            var id = editing.id;
            finish(true);
            send('row', { i: id, b: r[1] });
          });
          box.appendChild(btn);
        });
      }
      e.ta = ta;
      e.hint = hint;
      e.box = box;
      hint.textContent = hintFor(e);
      wire(e);
      return box;
    }

    // value 가 있으면(외부 쓰기로 다시 그린 뒤 되살리는 글) 원문 조각 입력칸으로 연다. 없으면 그 자리
    // 편집을 먼저 시도하고, DOM 과 원문을 맞출 수 없는 블록만 원문 조각 입력칸으로 연다.
    function openEditor(id, value, at, atStart) {
      var el = els[id];
      var b = B[id];
      if (!el || !b || !(b[3] & F_EDIT) || I.readonly) return;
      if (fold) fold.reveal(el);
      if (value == null && openLive(id, at, atStart)) return;
      var e = { id: id, kind: b[0], orig: b[1], isNew: false, el: el };
      var box = buildBox(e);
      if (e.kind === 'li') {
        var nodes = ownNodes(el);
        var target = nodes.length === 1 && nodes[0].nodeType === 1 && nodes[0].tagName === 'P' ? nodes[0] : null;
        if (target) {
          e.saved = target.innerHTML;
          e.host = target;
          target.innerHTML = '';
          target.appendChild(box);
        } else {
          var hold = document.createElement('span');
          hold.style.display = 'none';
          var anchor = nodes.length ? nodes[0] : null;
          el.insertBefore(hold, anchor || firstBlockChild(el));
          nodes.forEach(function (n) { hold.appendChild(n); });
          el.insertBefore(box, hold);
          e.hold = hold;
        }
      } else {
        e.saved = el.innerHTML;
        e.host = el;
        el.innerHTML = '';
        el.appendChild(box);
      }
      start(e, value);
    }

    function firstBlockChild(li) {
      for (var n = li.firstChild; n; n = n.nextSibling) {
        if (n.nodeType === 1 && BLOCKISH.test(n.tagName)) return n;
      }
      return null;
    }

    // ---- 그 자리 편집 (docs/surfaces/mdedit/index.md#정렬되는-블록은-렌더된-자리에서-고친다) ----
    // 렌더된 블록을 그대로 편집한다. 무손실의 근거는 정렬이다 — 블록을 열 때 DOM 을 서식 표기와
    // 함께 직렬화한 글(D)을 원문 조각(S)과 글자 단위로 맞춘다(같은 글자이거나 렌더러의 스마트 구두점
    // 치환만 허용). 맞으면 그 자리에서 고치고, 적용할 때는 D 에서 바뀐 구간만 정렬표로 S 의 위치에
    // 옮겨 바꾼다 — 손대지 않은 원문 바이트는 그대로다. 안 맞으면 원문 조각 입력칸으로 연다.
    var NAV_LINK = '#tasty-nav:link:';
    var SMART = { '\u2026': '...', '\u2014': '---', '\u2013': '--', '\u201c': '"', '\u201d': '"', '\u2018': "'", '\u2019': "'" };

    function decodeDest(h) {
      if (h.indexOf(NAV_LINK) !== 0) return h;
      try { return decodeURIComponent(h.slice(NAV_LINK.length)); } catch (x) { return null; }
    }

    // DOM → markdown(서식 표기 포함). 되돌리는 법을 모르는 요소가 있으면 null.
    function ser(node, top) {
      var out = '';
      for (var n = node.firstChild; n; n = n.nextSibling) {
        if (n.nodeType === 3) { out += n.nodeValue; continue; }
        if (n.nodeType !== 1 || n.classList.contains('mde-fold')) continue;
        var tag = n.tagName;
        if (tag === 'BR') { if (!top || n.nextSibling) out += '\n'; continue; }
        if (tag === 'INPUT') continue;
        if (tag === 'CODE') { out += '`' + n.textContent + '`'; continue; }
        var inner = ser(n, false);
        if (inner === null) return null;
        if (tag === 'STRONG' || tag === 'B') out += '**' + inner + '**';
        else if (tag === 'EM' || tag === 'I') out += '*' + inner + '*';
        else if (tag === 'DEL' || tag === 'S') out += '~~' + inner + '~~';
        else if (tag === 'A') {
          // 연 순간 맨 주소였던 링크(자동 링크)는 고친 뒤에도 맨 주소로 쓴다 — 글이 주소와 달라졌다고
          // `[글](주소)` 로 바꾸면 사용자가 친 글자 밖의 원문이 바뀐다.
          var dest = decodeDest(n.getAttribute('href') || '');
          if (dest === null) return null;
          var bare = n.hasAttribute('data-mde-bare') || (n.getAttribute('data-mde-link') === null && inner === dest);
          out += bare ? inner : '[' + inner + '](' + dest + ')';
        } else return null;
      }
      return out;
    }

    // D 의 경계마다 S 의 위치. 맞출 수 없으면 null.
    function align(S, D) {
      var m = new Array(D.length + 1), i = 0, j = 0;
      while (i < D.length) {
        var c = D.charAt(i);
        if (S.charAt(j) === c) { m[i++] = j++; continue; }
        var r = SMART[c];
        if (r && S.substr(j, r.length) === r) { m[i++] = j; j += r.length; continue; }
        return null;
      }
      if (j !== S.length) return null;
      m[D.length] = S.length;
      return m;
    }

    // 렌더러가 블록 앞뒤에 둔 줄바꿈(체크박스 뒤 · 하위 목록 앞)은 원문 범위 밖이다 — 편집 전후에
    // 똑같이 걷어 내므로 정렬·접합이 어긋나지 않는다.
    // WebKit 은 편집하며 보이는 공백을 지키려고 NBSP 를 넣는다 — 공백으로 되돌린다. 원문에 진짜 NBSP 가
    // 있는 블록은 이 때문에 정렬이 안 맞아 원문 조각 입력칸으로 열린다(안전한 쪽).
    function serHost(host) {
      var d = ser(host, true);
      return d === null ? null : d.replace(/\u00a0/g, ' ').replace(/^[ \t\n]+/, '').replace(/\n+$/, '');
    }

    // 고친 DOM 을 원문으로 — 바뀐 구간만 S 에 옮긴다.
    function liveSource(e) {
      var D2 = serHost(e.host);
      if (D2 === null) return null;
      var D = e.d0, n = Math.min(D.length, D2.length), a = 0, b = 0;
      while (a < n && D.charCodeAt(a) === D2.charCodeAt(a)) a++;
      while (b < n - a && D.charCodeAt(D.length - 1 - b) === D2.charCodeAt(D2.length - 1 - b)) b++;
      return e.orig.slice(0, e.map[a]) + D2.slice(a, D2.length - b) + e.orig.slice(e.map[D.length - b]);
    }

    // 항목은 자기 첫 줄만 편집한다 — 하위 목록·체크박스는 밖에 둔다.
    // 체크박스와 그 뒤 줄바꿈(렌더러가 `<input>` 뒤에 붙이는 "\n")은 편집 범위 밖에 둔다 — 안에 두면
    // 편집 중 pre-wrap 에서 그 줄바꿈이 보여 글이 체크박스 아랫줄로 내려간다. 느슨한 목록은 체크박스가
    // 첫 문단 안에 있으므로 그 문단 안에서 같은 일을 한다.
    function liveHost(el, kind) {
      if (kind !== 'li') return el;
      var box = el, nodes = ownNodes(el);
      var isEl = function (n, test) { return n && n.nodeType === 1 && test(n); };
      if (nodes.length === 1 && isEl(nodes[0], function (n) { return n.tagName === 'P'; })) {
        box = nodes[0];
        if (!box.querySelector(':scope > input')) return box;
        nodes = Array.prototype.filter.call(box.childNodes, function (n) { return !isEl(n, function (x) { return x.tagName === 'INPUT'; }); });
      }
      // 앞뒤의 공백뿐인 글 노드(체크박스 뒤 · 하위 목록 앞 줄바꿈)는 감싸지 않는다 — 편집 중 pre-wrap 에서
      // 빈 줄로 보이지 않게.
      var blank = function (n) { return n && n.nodeType === 3 && !/\S/.test(n.nodeValue); };
      while (blank(nodes[0])) nodes.shift();
      while (blank(nodes[nodes.length - 1])) nodes.pop();
      if (nodes.length === 1 && isEl(nodes[0], function (n) { return n.classList.contains('mde-own'); })) return nodes[0];
      // 글 노드 머리·꼬리의 공백도 밖으로 — 직렬화는 어차피 걷어 내므로 정렬은 그대로다.
      if (nodes.length && nodes[0].nodeType === 3) {
        var lead = /^[ \t\n]+/.exec(nodes[0].nodeValue);
        if (lead) nodes[0] = nodes[0].splitText(lead[0].length);
      }
      var last = nodes[nodes.length - 1];
      if (last && last.nodeType === 3) {
        var tail = /[ \t\n]+$/.exec(last.nodeValue);
        if (tail) last.splitText(last.nodeValue.length - tail[0].length);
      }
      var span = document.createElement('span');
      span.className = 'mde-own';
      box.insertBefore(span, nodes.length ? nodes[0] : (box === el ? firstBlockChild(el) : null));
      nodes.forEach(function (n) { span.appendChild(n); });
      return span;
    }

    function placeCaret(host, at, atStart) {
      var sel = window.getSelection();
      var r = null;
      if (at && document.caretRangeFromPoint) r = document.caretRangeFromPoint(at.x, at.y);
      if (!r || !host.contains(r.startContainer)) {
        r = document.createRange();
        r.selectNodeContents(host);
        r.collapse(!!atStart);
      }
      sel.removeAllRanges();
      sel.addRange(r);
    }

    function openLive(id, at, atStart) {
      var b = B[id], el = els[id];
      var host = liveHost(el, b[0]);
      var saved = host.innerHTML;
      // 링크마다 연 순간의 모양(맨 주소 · [글](주소))을 고정한다.
      Array.prototype.forEach.call(host.querySelectorAll('a'), function (a) {
        var inner = ser(a, false), dest = decodeDest(a.getAttribute('href') || '');
        if (inner !== null && inner === dest) a.setAttribute('data-mde-bare', '');
        else a.setAttribute('data-mde-link', '');
      });
      var d0 = serHost(host);
      var map = d0 === null ? null : align(b[1], d0);
      if (!map) { host.innerHTML = saved; return false; }
      var e = { id: id, kind: b[0], orig: b[1], isNew: false, el: el, host: host, live: true,
                d0: d0, map: map, saved: saved };
      host.setAttribute('contenteditable', 'plaintext-only');
      if (host.contentEditable !== 'plaintext-only') host.setAttribute('contenteditable', 'true');
      host.setAttribute('spellcheck', 'false');
      host.classList.add('mde-live');
      editing = e;
      wireLive(e);
      if (window.__mdeFmt) window.__mdeFmt();
      host.focus();
      placeCaret(host, at, atStart);
      if (e.kind === 'td') rowBar(e);
      return true;
    }

    // 표 셀 — 행 추가 단추를 셀 아래에 띄운다.
    function rowBar(e) {
      var bar = document.createElement('div');
      bar.className = 'mde-rowbar';
      var head = B[e.id][3] & F_HEAD;
      [['row_above', false], ['row_below', true]].forEach(function (r) {
        if (head && !r[1]) return;
        var btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'mde-inline-btn';
        btn.textContent = T[r[0]] || r[0];
        btn.addEventListener('mousedown', function (ev) { ev.preventDefault(); });
        btn.addEventListener('click', function (ev) {
          ev.preventDefault();
          ev.stopPropagation();
          addRow(e, r[1]);
        });
        bar.appendChild(btn);
      });
      var rect = e.host.getBoundingClientRect();
      bar.style.top = (rect.bottom + window.scrollY) + 'px';
      bar.style.left = (rect.left + window.scrollX) + 'px';
      document.body.appendChild(bar);
      e.bar = bar;
    }

    // 고친 것이 있으면 먼저 적용하고 행을 넣는다 — 셀 글을 바꿔도 블록 번호는 그대로다.
    function addRow(e, below) {
      if (editing !== e) return;
      var v = currentValue(e);
      finish(true);
      if (v !== null && v !== e.orig) send('blk', { i: e.id, o: e.orig, t: v, then: '' });
      send('row', { i: e.id, b: below });
    }

    function wireLive(e) {
      var host = e.host;
      e.onKey = function (ev) {
        if (ev.isComposing || ev.keyCode === 229) return;
        if (menu && menuKey(ev)) return;
        if (ev.key === 'Escape') { ev.preventDefault(); finish(false); return; }
        if (ev.key === 'Enter') { ev.preventDefault(); onEnter(e, ev); return; }
        if ((ev.key === 'ArrowUp' || ev.key === 'ArrowDown') && !ev.shiftKey && !ev.altKey && !ev.metaKey && !ev.ctrlKey) {
          if (moveVert(e, ev.key === 'ArrowUp')) ev.preventDefault();
        }
      };
      e.onInput = function () { updateMenu(e); saveDraft(); pend(); };
      e.onBlur = function () {
        setTimeout(function () {
          if (editing === e && !host.contains(document.activeElement)) commit('');
        }, 0);
      };
      e.onPaste = function (ev) {
        // 서식 붙여넣기 대신 글만 — plaintext-only 를 모르는 엔진도 같게.
        var t = ev.clipboardData && ev.clipboardData.getData('text/plain');
        if (t == null) return;
        ev.preventDefault();
        document.execCommand('insertText', false, t.replace(/\r?\n/g, ' '));
      };
      host.addEventListener('keydown', e.onKey);
      host.addEventListener('input', e.onInput);
      host.addEventListener('blur', e.onBlur);
      host.addEventListener('paste', e.onPaste);
    }

    function unwireLive(e) {
      var host = e.host;
      host.removeEventListener('keydown', e.onKey);
      host.removeEventListener('input', e.onInput);
      host.removeEventListener('blur', e.onBlur);
      host.removeEventListener('paste', e.onPaste);
      host.removeAttribute('contenteditable');
      host.removeAttribute('spellcheck');
      host.classList.remove('mde-live');
      if (e.bar) e.bar.remove();
    }

    // 지금 편집 중인 블록의 원문 — 그 자리 편집은 정렬로, 입력칸은 그 값.
    function currentValue(e) { return e.live ? liveSource(e) : e.ta.value; }

    // 블록 뒤(id 가 null 이면 문서 끝)에 새 줄 입력칸.
    function openNewLine(afterId, value) {
      if (I.readonly) return;
      if (fold && afterId != null) fold.reveal(els[afterId]);
      var e = { id: afterId, kind: 'new', orig: '', isNew: true };
      var box = buildBox(e);
      box.classList.add('mde-newline');
      if (afterId == null || !els[afterId]) {
        body.insertBefore(box, add);
      } else if (B[afterId][0] === 'li') {
        // 항목 뒤(빈 항목에서 목록을 빠져나온 자리) — 그 항목 바로 아래에 보인다. 쓴 문단은 목록을 나눈다.
        els[afterId].parentNode.insertBefore(box, els[afterId].nextSibling);
      } else {
        var anchor = els[afterId];
        while (anchor.parentNode && anchor.parentNode !== body) anchor = anchor.parentNode;
        body.insertBefore(box, anchor.nextSibling);
      }
      start(e, value);
    }

    // 최상위 단위 뒤의 새 줄 입력칸 — 손잡이 `+`. 적용은 단위 기준(`uins`)이다.
    function openNewLineAfterUnit(u, value) {
      if (I.readonly || !unitEl[u]) return;
      var e = { id: null, unit: u, kind: 'new', orig: '', isNew: true };
      var box = buildBox(e);
      box.classList.add('mde-newline');
      body.insertBefore(box, unitEl[u].nextSibling);
      start(e, value);
    }

    function start(e, value) {
      editing = e;
      if (value != null) e.ta.value = value;
      autosize(e.ta);
      e.ta.focus();
      var end = e.ta.value.length;
      e.ta.setSelectionRange(end, end);
      if (e.box.scrollIntoView) e.box.scrollIntoView({ block: 'nearest' });
      saveDraft();
      pend();
    }

    // 편집기를 걷고 원래 요소를 되돌린다. `keepShown` 이면 plugin 이 다시 그릴 때까지 글을 둔다.
    function finish(keepShown) {
      var e = editing;
      editing = null;
      closeMenu();
      clearDraft();
      if (window.__mdeFmt) window.__mdeFmt();
      // 고친 것을 버렸다 — 적용(keepShown)이면 plugin 이 적용하며 스스로 지우므로 여기서도 잊는다.
      if (keepShown) pendSent = false;
      else pend();
      if (!e) return;
      if (e.live) {
        unwireLive(e);
        if (!keepShown) e.host.innerHTML = e.saved;
        return;
      }
      if (keepShown) { e.ta.readOnly = true; return; }
      if (e.isNew) { e.box.remove(); return; }
      if (e.hold) {
        while (e.hold.firstChild) e.el.insertBefore(e.hold.firstChild, e.hold);
        e.hold.remove();
        e.box.remove();
      } else if (e.host) {
        e.host.innerHTML = e.saved;
      }
    }

    // then: '' | 'item' | 'line'
    function commit(then) {
      var e = editing;
      if (!e) return;
      var v = currentValue(e);
      if (v === null) { finish(false); return; }
      if (e.isNew) {
        if (!v.trim()) { finish(false); return; }
        var p = { t: v, c: then === 'line' };
        if (e.id != null) p.a = e.id;
        finish(true);
        if (e.unit != null) send('uins', { u: e.unit, h: U[e.unit][3], t: v, c: then === 'line' });
        else send('ins', p);
        return;
      }
      if (v === e.orig && !then) { finish(false); return; }
      if (!(B[e.id][3] & F_MULTI)) v = v.replace(/\n/g, ' ');
      finish(true);
      send('blk', { i: e.id, o: e.orig, t: v, then: then || '' });
    }

    commitInline = function () { commit(''); };

    function changed(e) {
      var v = currentValue(e);
      return v !== null && v !== e.orig;
    }

    // 열린 블록에 적용 전 고침이 있는지 plugin 에 알린다(바뀔 때만) — plugin 의 dirty 가 그것을 담아
    // 저장 단추를 켠다(docs/surfaces/mdedit/index.md#적용-전-고침도-dirty-로-센다). 버퍼는 적용할 때 바뀐다. 새 문서에는 열린 편집기가 없으므로 처음
    // 값은 거짓이고, plugin 도 다시 그릴 때 거짓으로 되돌린다.
    var pendSent = false;
    function pend() {
      var e = editing;
      var p = !!e && (e.isNew ? !!(e.ta && e.ta.value.trim()) : changed(e));
      if (p === pendSent) return;
      pendSent = p;
      send('pend', { p: p });
    }

    function onEnter(e, ev) {
      var b = e.isNew ? null : B[e.id];
      if (!e.live && (e.multi || (ev.shiftKey && (e.isNew || (b[3] & F_MULTI))))) {
        if (ev.metaKey || ev.ctrlKey) { ev.preventDefault(); commit(e.isNew ? 'line' : ''); }
        return;
      }
      ev.preventDefault();
      if (e.isNew) { commit(e.ta.value.trim() ? 'line' : ''); return; }
      if (e.live && ev.shiftKey) {
        // 블록 안 줄바꿈은 문단만 — 원문 줄바꿈이 렌더 글자와 그대로 맞는 블록은 문단뿐이다(항목·제목의
        // 둘째 줄은 원문 표기가 렌더 글자와 갈려 다음에 열 때 정렬이 안 맞는다).
        if (e.kind === 'p') insertNewline(e);
        return;
      }
      if (e.kind === 'li') {
        var cur = currentValue(e);
        if (cur !== null && !cur.trim() && (b[3] & F_EMPTY_ITEM)) {
          var id = e.id;
          finish(true);
          send('unitem', { i: id });
          return;
        }
        if (e.live && split(e)) return;
        commit('item');
      } else if ((e.kind === 'p' || e.kind === 'h') && (b[3] & F_AFTER)) {
        if (e.live && split(e)) return;
        commit('line');
      } else {
        commit('');
      }
    }

    // 커서 자리에 줄바꿈 글자를 넣는다. `insertText` 로는 plaintext-only 편집이 줄바꿈을 안 넣는다(Chrome ·
    // WebKit 실측) — 글 노드를 직접 넣고 입력 처리(메뉴 · 초안 · 고침 알림)를 부른다.
    function insertNewline(e) {
      var sel = window.getSelection();
      if (!sel.rangeCount) return;
      if (!sel.getRangeAt(0).collapsed) document.execCommand('delete');
      var r = sel.getRangeAt(0);
      if (!e.host.contains(r.startContainer)) return;
      var t = document.createTextNode('\n');
      r.insertNode(t);
      var c = document.createRange();
      c.setStartAfter(t);
      c.collapse(true);
      sel.removeAllRanges();
      sel.addRange(c);
      if (e.onInput) e.onInput();
    }

    // ---- Enter 로 나누기 (docs/surfaces/mdedit/index.md#enter-는-커서-자리에서-나눈다) ----
    // 커서 자리에 표지 글자를 잠깐 끼워 원문으로 직렬화한다 — 그 자리 편집의 정렬 접합을 그대로 타므로
    // 표지 앞이 앞쪽 원문, 뒤가 뒤쪽 원문이다(합치면 원문 그대로). 커서가 서식(굵게·링크·코드) 안이면
    // 표기가 갈리지 않게 그 서식 끝에서 나눈다.
    var SPLIT_MARK = '\uE000';
    function splitAtCaret(e) {
      var sel = window.getSelection();
      if (!sel.rangeCount) return null;
      var r = sel.getRangeAt(0);
      if (!e.host.contains(r.startContainer)) return null;
      if (!r.collapsed) { document.execCommand('delete'); r = sel.getRangeAt(0); }
      var top = null;
      for (var x = r.startContainer.nodeType === 1 ? r.startContainer : r.startContainer.parentNode; x && x !== e.host; x = x.parentNode) top = x;
      var at = r.cloneRange();
      if (top) { at = document.createRange(); at.setStartAfter(top); at.collapse(true); }
      var mark = document.createTextNode(SPLIT_MARK);
      at.insertNode(mark);
      var v = liveSource(e);
      mark.parentNode.removeChild(mark);
      if (v === null) return null;
      var k = v.indexOf(SPLIT_MARK);
      if (k < 0 || v.indexOf(SPLIT_MARK, k + 1) >= 0) return null;
      return { before: v.slice(0, k), rest: v.slice(k + 1) };
    }

    // 나누기를 보낸다. 문단·제목을 머리에서 나누면(앞쪽이 빔) 종전대로 적용 + 새 줄로 둔다.
    function split(e) {
      var s = splitAtCaret(e);
      if (!s) return false;
      if (e.kind !== 'li' && !s.before.trim()) return false;
      var before = s.before;
      if (!(B[e.id][3] & F_MULTI)) before = before.replace(/\n/g, ' ');
      var id = e.id, orig = e.orig;
      finish(true);
      send('split', { i: id, o: orig, t: before, r: s.rest });
      return true;
    }

    // ---- 위·아래 방향키로 블록 사이 이동 ----
    // 첫 줄에서 위 · 마지막 줄에서 아래를 누르면 이웃 편집 단위를 그 자리 편집으로 연다(가로 위치 유지).
    // 고친 것이 있으면 적용하고, 다시 그린 문서에서 그 이웃을 연다. 표 칸은 이동 대상이 아니다.
    var navKey = stateKey + ':nav';
    function neighbor(id, up) {
      for (var k = id + (up ? -1 : 1); k >= 0 && k < B.length; k += up ? -1 : 1) {
        var n = els[k];
        if (n && (B[k][3] & F_EDIT) && B[k][0] !== 'td' && n.getClientRects().length) return k;
      }
      return null;
    }
    function atEdge(e, up) {
      var sel = window.getSelection();
      if (!sel.rangeCount || !sel.getRangeAt(0).collapsed) return false;
      var hr = e.host.getClientRects();
      if (!hr.length) return true;
      var cr = sel.getRangeAt(0).getClientRects();
      var c = cr.length ? cr[0] : null;
      if (!c || !c.height) {
        var t = caretText(e);
        return !!t && (up ? t.c === 0 : t.c >= t.v.length);
      }
      return up ? c.top < hr[0].top + c.height / 2 : c.bottom > hr[hr.length - 1].bottom - c.height / 2;
    }
    function caretX(e) {
      var cr = window.getSelection().getRangeAt(0).getClientRects();
      return cr.length ? cr[0].left : e.host.getBoundingClientRect().left;
    }
    function openAtX(k, x, up) {
      var n = els[k];
      if (n.scrollIntoView) n.scrollIntoView({ block: 'nearest' });
      var tr = n.getBoundingClientRect();
      openEditor(k, null, { x: x, y: up ? tr.bottom - 2 : tr.top + 2 }, !up);
    }
    function moveVert(e, up) {
      if (e.kind === 'td') return false;
      var k = neighbor(e.id, up);
      if (k == null || !atEdge(e, up)) return false;
      var x = caretX(e);
      if (changed(e)) {
        try { sessionStorage.setItem(navKey, JSON.stringify({ id: k, x: x, up: up, o: B[k][1] })); } catch (q) {}
        commit('');
        return true;
      }
      finish(false);
      openAtX(k, x, up);
      return true;
    }

    function wire(e) {
      var ta = e.ta;
      ta.addEventListener('keydown', function (ev) {
        if (ev.isComposing || ev.keyCode === 229) return;
        if (menu && menuKey(ev)) return;
        if (ev.key === 'Escape') { ev.preventDefault(); finish(false); return; }
        if (ev.key === 'Enter') onEnter(e, ev);
      });
      ta.addEventListener('input', function () {
        autosize(ta);
        updateMenu(e);
        saveDraft();
        pend();
      });
      ta.addEventListener('blur', function () {
        setTimeout(function () {
          if (editing === e && document.activeElement !== ta) commit('');
        }, 0);
      });
    }

    // ---- 블록 삽입 메뉴 ("/") ----
    // [id, 라벨 키, 원문 틀, 커서 위치(틀 안), 여러 줄 틀, 머리 표지(기존 문단에서도 쓴다)]
    var ITEMS = [
      ['paragraph', 'slash_paragraph', '', 0, false, true],
      ['h1', 'slash_h1', '# ', 2, false, true],
      ['h2', 'slash_h2', '## ', 3, false, true],
      ['h3', 'slash_h3', '### ', 4, false, true],
      ['bullet', 'slash_bullet', '- ', 2, false, true],
      ['numbered', 'slash_numbered', '1. ', 3, false, true],
      ['task', 'slash_task', '- [ ] ', 6, false, true],
      ['quote', 'slash_quote', '> ', 2, false, true],
      ['code', 'slash_code', '```\n\n```', 4, true, false],
      ['table', 'slash_table', '|   |   |\n| --- | --- |\n|   |   |', 2, true, false],
      ['divider', 'slash_divider', '---', 3, false, false]
    ];

    // 그 자리 편집에서는 글(textContent)과 커서까지의 글 길이로 본다.
    function caretText(e) {
      if (!e.live) return { v: e.ta.value, c: e.ta.selectionStart, collapsed: e.ta.selectionStart === e.ta.selectionEnd };
      var sel = window.getSelection();
      if (!sel.rangeCount) return null;
      var r = sel.getRangeAt(0);
      var pre = document.createRange();
      pre.selectNodeContents(e.host);
      pre.setEnd(r.startContainer, r.startOffset);
      return { v: e.host.textContent, c: pre.toString().length, collapsed: r.collapsed };
    }

    function slashAt(e) {
      if (!(e.isNew || e.kind === 'p')) return null;
      var ct = caretText(e);
      if (!ct || !ct.collapsed) return null;
      var v = ct.v, c = ct.c;
      var ls = v.lastIndexOf('\n', c - 1) + 1;
      if (v.charAt(ls) !== '/') return null;
      var q = v.slice(ls + 1, c);
      if (/\s/.test(q)) return null;
      return { ls: ls, c: c, q: q.toLowerCase(), raw: q };
    }

    function updateMenu(e) {
      var s = slashAt(e);
      if (!s) { closeMenu(); return; }
      var list = ITEMS.filter(function (it) {
        if (!e.isNew && !it[5]) return false;
        var label = (T[it[1]] || '').toLowerCase();
        return !s.q || it[0].indexOf(s.q) === 0 || label.indexOf(s.q) >= 0;
      });
      if (!menu) {
        menu = document.createElement('div');
        menu.className = 'mde-slash';
        if (e.live) {
          // 블록 아래에 띄운다 — 블록 안에 넣으면 그 글의 일부가 된다.
          menu.classList.add('mde-slash-float');
          var rect = e.host.getBoundingClientRect();
          menu.style.top = (rect.bottom + window.scrollY) + 'px';
          menu.style.left = (rect.left + window.scrollX) + 'px';
          document.body.appendChild(menu);
        } else {
          e.box.appendChild(menu);
        }
      }
      menu.innerHTML = '';
      menu.sel = 0;
      menu.list = list;
      menu.at = s;
      if (!list.length) {
        var none = document.createElement('div');
        none.className = 'mde-slash-none';
        none.textContent = T.no_match || '';
        menu.appendChild(none);
        placeMenu(e);
        return;
      }
      list.forEach(function (it, i) {
        var row = document.createElement('div');
        row.className = 'mde-slash-item' + (i === 0 ? ' on' : '');
        row.textContent = T[it[1]] || it[0];
        row.addEventListener('mousedown', function (ev) { ev.preventDefault(); });
        row.addEventListener('click', function (ev) { ev.preventDefault(); choose(e, it); });
        menu.appendChild(row);
      });
      placeMenu(e);
    }

    // 입력칸이 창 아래쪽이면 아래로 펼친 메뉴가 화면 밖으로 잘린다 — 위쪽 공간이 더 넓으면 위로 편다.
    function placeMenu(e) {
      if (e.live) return;
      menu.classList.remove('up');
      var below = window.innerHeight - e.box.getBoundingClientRect().bottom;
      var above = e.box.getBoundingClientRect().top;
      if (menu.getBoundingClientRect().height > below && above > below) menu.classList.add('up');
    }

    function closeMenu() { if (menu) { menu.remove(); menu = null; } }

    function menuKey(ev) {
      var n = menu.list ? menu.list.length : 0;
      if (ev.key === 'Escape') { ev.preventDefault(); closeMenu(); return true; }
      if (!n) return false;
      if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
        ev.preventDefault();
        menu.sel = (menu.sel + (ev.key === 'ArrowDown' ? 1 : n - 1)) % n;
        Array.prototype.forEach.call(menu.children, function (r, i) { r.classList.toggle('on', i === menu.sel); });
        return true;
      }
      if (ev.key === 'Enter' || ev.key === 'Tab') {
        ev.preventDefault();
        choose(editing, menu.list[menu.sel]);
        return true;
      }
      return false;
    }

    function choose(e, it) {
      if (!e || !menu) return;
      var s = menu.at;
      closeMenu();
      if (e.live) {
        // 문단의 줄 머리 "/검색어" 를 고른 표지로 바꿔 곧바로 적용한다(블록 종류가 바뀌어 다시 그린다).
        var src = liveSource(e);
        if (src === null) return;
        var tok = '/' + s.raw;
        var at = -1;
        for (var k = src.indexOf(tok); k >= 0; k = src.indexOf(tok, k + 1)) {
          if (k === 0 || src.charAt(k - 1) === '\n') { at = k; break; }
        }
        if (at < 0) return;
        var next = src.slice(0, at) + it[2] + src.slice(at + tok.length);
        finish(true);
        send('blk', { i: e.id, o: e.orig, t: next, then: '' });
        return;
      }
      var v = e.ta.value;
      e.ta.value = v.slice(0, s.ls) + it[2] + v.slice(s.c);
      var caret = s.ls + it[3];
      e.ta.setSelectionRange(caret, caret);
      if (it[4]) { e.multi = true; e.hint.textContent = hintFor(e); }
      autosize(e.ta);
      saveDraft();
      if (it[0] === 'divider' && e.isNew) commit('line');
    }

    // ---- 클릭으로 열기 ----
    body.addEventListener('click', function (ev) {
      var t = ev.target;
      if (!t || !t.closest || I.readonly) return;
      if (t === add) {
        ev.preventDefault();
        if (editing && changed(editing)) { commit(''); return; }
        finish(false);
        openNewLine(null);
        return;
      }
      if (t.closest('.mde-inline')) return;
      // 편집 중인 블록 안 — 커서만 옮긴다(링크도 따라가지 않는다).
      if (editing && editing.live && editing.host.contains(t)) {
        if (t.closest('a')) ev.preventDefault();
        return;
      }
      if (t.closest('a,input,button,summary,img,video,audio')) return;
      var sel = window.getSelection && window.getSelection();
      if (sel && !sel.isCollapsed) return;
      var host = t.closest('[data-mde]');
      if (!host) return;
      // 항목 안의 코드 블록·인용·표·하위 목록을 누른 것은 그 항목의 첫 줄이 아니다.
      var inner = t.closest('pre,blockquote,table,ul,ol,div,details,figure');
      if (inner && inner !== host && host.contains(inner)) return;
      var id = parseInt(host.getAttribute('data-mde'), 10);
      if (!(B[id][3] & F_EDIT)) return;
      ev.preventDefault();
      if (editing) {
        // 고친 것이 있으면 먼저 적용한다(문서를 다시 그리므로 이 클릭은 거기서 끝난다).
        if (changed(editing)) { commit(''); return; }
        finish(false);
      }
      openEditor(id, null, { x: ev.clientX, y: ev.clientY });
    });


    // ---- 손잡이 · 블록 메뉴 · 끌어 옮기기 (docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다) ----
    var FMTS = ['paragraph', 'h1', 'h2', 'h3', 'bullet', 'numbered', 'task', 'quote'];
    function unitLabel(u) {
      var k = U[u][0];
      if (k === 'p') return T.slash_paragraph;
      if (/^h[1-3]$/.test(k)) return T['slash_' + k];
      if (/^h[4-6]$/.test(k)) return T['fmt_' + k];
      if (k === 'li') return U[u][2] ? T.slash_task : (UL[U[u][1]] && UL[U[u][1]][0] ? T.slash_numbered : T.slash_bullet);
      return { quote: T.slash_quote, code: T.slash_code, table: T.slash_table, hr: T.slash_divider }[k] || '';
    }
    function unitFormat(u) {
      var k = U[u][0];
      if (k === 'p') return 'paragraph';
      if (k === 'li') return U[u][2] ? 'task' : (UL[U[u][1]] && UL[U[u][1]][0] ? 'numbered' : 'bullet');
      return k;
    }
    function editLabel(e) {
      if (!e || e.isNew) return '';
      if (e.kind === 'p') return T.slash_paragraph;
      if (e.kind === 'td') return T.slash_table;
      var mu = e.el.getAttribute('data-mu');
      if (mu != null) return unitLabel(+mu);
      if (e.kind === 'h') { var n = e.el.tagName.charAt(1); return n <= 3 ? T['slash_h' + n] : T['fmt_h' + n]; }
      return '';
    }
    var picked = null;
    function showFmt() {
      var slot = document.getElementById('mde-blockfmt');
      if (!slot) return;
      slot.textContent = picked != null ? unitLabel(picked) : editLabel(editing);
    }
    window.__mdeFmt = showFmt;
    function pick(u) {
      unpick();
      if (u == null || !unitEl[u]) return;
      picked = u;
      unitEl[u].classList.add('mde-picked');
      showFmt();
    }
    function unpick() {
      if (picked != null && unitEl[picked]) unitEl[picked].classList.remove('mde-picked');
      picked = null;
      showFmt();
    }

    // 같은 그릇의 보이는 자리 — [{key: {bu}|{bl}, el, u}] (요소가 짝지어진 것만).
    function siblingsOf(u) {
      if (U[u][1] >= 0) {
        var out = [];
        for (var i = 0; i < U.length; i++) if (U[i][1] === U[u][1] && unitEl[i]) out.push({ key: { bu: i }, el: unitEl[i], u: i });
        return out;
      }
      return UT.filter(function (t) { return t[0] === 1 ? !!listEl[t[1]] : (U[t[1]][0] !== 'none' && !!unitEl[t[1]]); })
        .map(function (t) { return t[0] === 1 ? { key: { bl: t[1] }, el: listEl[t[1]] } : { key: { bu: t[1] }, el: unitEl[t[1]], u: t[1] }; });
    }

    // 손잡이를 쓰기 전에 열린 편집을 정리한다 — 고친 것이 있으면 적용만 하고 멈춘다(다시 그린 뒤 다시).
    function settle() {
      if (!editing) return true;
      if (changed(editing)) { commit(''); return false; }
      finish(false);
      return true;
    }
    function unitSend(u, a, extra) {
      var p = { u: u, h: U[u][3], a: a };
      if (extra) for (var k in extra) p[k] = extra[k];
      closeBlockMenu();
      send('unit', p);
    }

    var handles = document.createElement('div');
    handles.className = 'mde-handles';
    var hAdd = document.createElement('button');
    hAdd.type = 'button'; hAdd.className = 'mde-handle'; hAdd.textContent = '+'; hAdd.title = T.handle_add || '';
    var hDrag = document.createElement('button');
    hDrag.type = 'button'; hDrag.className = 'mde-handle mde-handle-drag'; hDrag.textContent = '⠿'; hDrag.title = T.handle_drag || '';
    handles.appendChild(hAdd); handles.appendChild(hDrag);
    document.body.appendChild(handles);
    var hoverU = null;

    function placeHandles(u) {
      var el = unitEl[u];
      var r = el.getBoundingClientRect();
      var cs = getComputedStyle(el);
      var fs = parseFloat(cs.fontSize) || 14;
      var lh = parseFloat(cs.lineHeight) || fs * 1.5;
      var space = parseFloat(getComputedStyle(body).getPropertyValue('--md-space-md')) || 8;
      var extra = space * 2 + (el.tagName === 'LI' ? fs * 1.5 : 0);
      handles.style.display = 'flex';
      handles.style.top = (r.top + window.scrollY + Math.max(0, (lh - handles.offsetHeight) / 2)) + 'px';
      handles.style.left = (r.left + window.scrollX - extra - handles.offsetWidth) + 'px';
    }
    function showHandles(u) {
      if (I.readonly || dragging) return;
      hoverU = u;
      placeHandles(u);
    }
    function hideHandles() {
      if (blockMenu || dragging) return;
      hoverU = null;
      handles.style.display = 'none';
    }
    // 블록에서 왼쪽 손잡이로 가는 동안 여백을 지나므로 숨김을 잠깐 늦춘다.
    var hideTimer = null;
    function cancelHide() { clearTimeout(hideTimer); hideTimer = null; }
    document.addEventListener('mousemove', function (ev) {
      if (dragging) return;
      var t = ev.target;
      if (!t || !t.closest) return;
      if (handles.contains(t) || (blockMenu && blockMenu.contains(t))) { cancelHide(); return; }
      var el = t.closest('[data-mu]');
      if (el && body.contains(el)) {
        cancelHide();
        var u = +el.getAttribute('data-mu');
        if (u !== hoverU) showHandles(u);
      } else if (!hideTimer) {
        hideTimer = setTimeout(function () { hideTimer = null; hideHandles(); }, 400);
      }
    });
    window.addEventListener('scroll', function () { if (hoverU != null && handles.style.display !== 'none') placeHandles(hoverU); });
    handles.addEventListener('mousedown', function (ev) { ev.preventDefault(); });

    // `+` — 항목이면 같은 수준 새 항목, 그 밖이면 그 뒤 새 줄 입력칸에 `/` 메뉴를 연다.
    hAdd.addEventListener('click', function (ev) {
      ev.preventDefault();
      var u = hoverU;
      if (u == null || !settle()) return;
      if (U[u][0] === 'li') { unitSend(u, 'new'); return; }
      openNewLineAfterUnit(u, '/');
      if (editing) updateMenu(editing);
    });

    // `⠿` — 누르고 끌면 옮기기, 그냥 떼면 블록 메뉴.
    var dragging = false, dragFrom = null, dropLine = null, dropAt = null;
    hDrag.addEventListener('mousedown', function (ev) {
      if (ev.button !== 0 || hoverU == null) return;
      ev.preventDefault();
      dragFrom = { u: hoverU, x: ev.clientX, y: ev.clientY };
      document.addEventListener('mousemove', onDragMove, true);
      document.addEventListener('mouseup', onDragEnd, true);
    });
    function onDragMove(ev) {
      if (!dragFrom) return;
      if (!dragging) {
        if (Math.abs(ev.clientY - dragFrom.y) + Math.abs(ev.clientX - dragFrom.x) < 5) return;
        if (!settle()) { dragFrom = null; return; }
        dragging = true;
        pick(dragFrom.u);
        dropLine = document.createElement('div');
        dropLine.className = 'mde-dropline';
        document.body.appendChild(dropLine);
      }
      ev.preventDefault();
      var sib = siblingsOf(dragFrom.u);
      var at = sib.length;
      for (var i = 0; i < sib.length; i++) {
        var r = sib[i].el.getBoundingClientRect();
        if (ev.clientY < r.top + r.height / 2) { at = i; break; }
      }
      dropAt = { sib: sib, at: at };
      var ref = at < sib.length ? sib[at].el.getBoundingClientRect() : sib[sib.length - 1].el.getBoundingClientRect();
      var own = unitEl[dragFrom.u].getBoundingClientRect();
      dropLine.style.top = ((at < sib.length ? ref.top : ref.bottom) + window.scrollY - 1) + 'px';
      dropLine.style.left = (own.left + window.scrollX) + 'px';
      dropLine.style.width = Math.max(40, own.width) + 'px';
    }
    function onDragEnd(ev) {
      document.removeEventListener('mousemove', onDragMove, true);
      document.removeEventListener('mouseup', onDragEnd, true);
      var from = dragFrom;
      dragFrom = null;
      if (!from) return;
      if (!dragging) { openBlockMenu(from.u); return; }
      dragging = false;
      if (dropLine) { dropLine.remove(); dropLine = null; }
      unpick();
      var d = dropAt;
      dropAt = null;
      if (!d) return;
      var me = -1;
      d.sib.forEach(function (x, i) { if (x.u === from.u) me = i; });
      if (d.at === me || d.at === me + 1) return;
      unitSend(from.u, 'move', d.at < d.sib.length ? d.sib[d.at].key : null);
    }

    var blockMenu = null;
    function closeBlockMenu() {
      if (!blockMenu) return;
      blockMenu.remove();
      blockMenu = null;
      unpick();
      hideHandles();
    }
    function openBlockMenu(u) {
      closeBlockMenu();
      if (!settle()) return;
      pick(u);
      var m = document.createElement('div');
      m.className = 'mde-slash mde-blockmenu';
      function row(label, on, act, cls) {
        var r = document.createElement('div');
        r.className = 'mde-slash-item' + (cls ? ' ' + cls : '') + (on ? '' : ' mde-disabled');
        r.textContent = label;
        r.addEventListener('mousedown', function (ev) { ev.preventDefault(); });
        r.addEventListener('click', function (ev) { ev.preventDefault(); ev.stopPropagation(); if (on) act(); });
        m.appendChild(r);
        return r;
      }
      var k = U[u][0];
      var fmts = k === 'li' ? ['bullet', 'numbered', 'task'] : (k === 'p' || /^h[1-6]$/.test(k)) ? FMTS : [];
      if (fmts.length) {
        var head = document.createElement('div');
        head.className = 'mde-blockmenu-head';
        head.textContent = T.menu_format || '';
        m.appendChild(head);
        var cur = unitFormat(u);
        fmts.forEach(function (f) {
          var it = ITEMS.filter(function (x) { return x[0] === f; })[0];
          var multiHead = U[u][5] && /^h[1-3]$/.test(f);
          row(T[it[1]] || f, f !== cur && !multiHead, function () { unitSend(u, 'fmt', { f: f }); }, f === cur ? 'on' : '');
        });
        var sep = document.createElement('div'); sep.className = 'mde-blockmenu-sep'; m.appendChild(sep);
      }
      var sib = siblingsOf(u), me = -1;
      sib.forEach(function (x, i) { if (x.u === u) me = i; });
      row(T.menu_up || 'up', me > 0, function () { unitSend(u, 'up'); });
      row(T.menu_down || 'down', me >= 0 && me < sib.length - 1, function () { unitSend(u, 'down'); });
      row(T.menu_dup || 'dup', true, function () { unitSend(u, 'dup'); });
      row(T.menu_delete || 'delete', true, function () { unitSend(u, 'del'); }, 'mde-danger');
      var r = hDrag.getBoundingClientRect();
      m.style.position = 'absolute';
      m.style.top = (r.bottom + window.scrollY + 2) + 'px';
      m.style.left = (r.left + window.scrollX) + 'px';
      document.body.appendChild(m);
      blockMenu = m;
    }
    document.addEventListener('mousedown', function (ev) {
      if (blockMenu && !blockMenu.contains(ev.target) && !handles.contains(ev.target)) closeBlockMenu();
    }, true);
    document.addEventListener('keydown', function (ev) {
      if (blockMenu && ev.key === 'Escape') { ev.preventDefault(); closeBlockMenu(); }
    });

    // 다시 그린 뒤: 다른 이유(외부 쓰기 병합 등)로 다시 그렸으면 쓰던 글을 되살리고, plugin 이
    // 넘긴 자리가 있으면 그 편집기를 연다.
    var draft = null;
    try { draft = JSON.parse(sessionStorage.getItem(draftKey) || 'null'); } catch (x) {}
    clearDraft();
    if (draft && draft.n && draft.u != null) {
      if (U[draft.u] && U[draft.u][3] === draft.uh) openNewLineAfterUnit(draft.u, draft.v);
    } else if (draft && draft.n) {
      if (draft.id == null) openNewLine(null, draft.v);
      else if (B[draft.id] && B[draft.id][1] === draft.ao) openNewLine(draft.id, draft.v);
    } else if (draft && B[draft.id] && B[draft.id][1] === draft.o) {
      openEditor(draft.id, draft.v);
    } else if (I.focus) {
      if (I.focus.after) openNewLine(I.focus.id);
      else openEditor(I.focus.id, null, null, I.focus.start);
    } else {
      // 방향키로 옮기다 적용한 경우 — 다시 그린 문서에서 그 이웃을 연다(원문 조각이 그대로일 때만).
      var nav = null;
      try { nav = JSON.parse(sessionStorage.getItem(navKey) || 'null'); } catch (x) {}
      if (nav && B[nav.id] && B[nav.id][1] === nav.o && els[nav.id]) openAtX(nav.id, nav.x, nav.up);
    }
    try { sessionStorage.removeItem(navKey); } catch (x) {}
  }
  setReadonly(I.readonly);
  window.addEventListener('beforeunload', function () { flush(); saveState(); });
})();
