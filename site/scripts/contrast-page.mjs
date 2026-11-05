/**
 * What the contrast check runs inside a page. pageLib is serialised into the page, so it reads nothing from this module.
 * Colors round per channel to 8 bits and a translucent layer composites source-over, as RULES.md says.
 */
export function pageLib() {
  const parse = (s) => {
    let m = /rgba?\(([^)]+)\)/.exec(s);
    if (m) { const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p[3] === undefined ? 1 : p[3]]; }
    m = /color\(srgb ([^)]+)\)/.exec(s);
    if (m) { const q = m[1].split(/[ /]+/).filter(Boolean).map(Number); return [q[0] * 255, q[1] * 255, q[2] * 255, q[3] === undefined ? 1 : q[3]]; }
    m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(s);
    if (m) return [parseInt(m[1].slice(0, 2), 16), parseInt(m[1].slice(2, 4), 16), parseInt(m[1].slice(4, 6), 16), m[2] ? parseInt(m[2], 16) / 255 : 1];
    return null;
  };
  const over = (t, u) => [0, 1, 2].map((i) => Math.floor(t[i] * t[3] + u[i] * (1 - t[3]) + 0.5)).concat(1);
  const lum = (c) => {
    const f = (v) => { const x = v / 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
  };
  const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
  const hex = (c) => "#" + c.slice(0, 3).map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
  const near = (a, b) => [0, 1, 2].every((i) => Math.abs(a[i] - b[i]) <= 1);

  const overlays = (e) => ["::before", "::after"].map((pseudo) => {
    const p = getComputedStyle(e, pseudo);
    if (p.content === "none" || p.position !== "absolute") return null;
    // A bar or a ring is not the ground. Only a pseudo-element that covers its box is.
    const box = e.getBoundingClientRect();
    if (parseFloat(p.width) < box.width - 1 || parseFloat(p.height) < box.height - 1) return null;
    const c = parse(p.backgroundColor);
    return c && c[3] > 0 ? c : null;
  }).filter(Boolean);
  const opacityOf = (el) => { let o = 1; for (let e = el; e; e = e.parentElement) o *= Number(getComputedStyle(e).opacity); return o; };
  const visible = (el) => {
    const s = getComputedStyle(el);
    if (s.visibility !== "visible" || s.display === "none") return false;
    // Fully transparent is not on screen. Any other opacity is measured, composited.
    if (opacityOf(el) === 0) return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  };
  // Each stroke and fill the svg itself paints, in document order. A nested svg reports its own. What a defs entry or a
  // use target holds is not walked: a use counts through its own inherited paint.
  const svgInks = (svg, onPaintServer) => {
    const inks = [];
    svg.querySelectorAll("path, circle, rect, line, polyline, polygon, ellipse, use, text").forEach((shape) => {
      if (shape.closest("svg") !== svg || shape.closest("defs, clipPath, mask, symbol, pattern, marker")) return;
      const s = getComputedStyle(shape);
      if (s.display === "none" || s.visibility !== "visible" || opacityOf(shape) === 0) return;
      const r = shape.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) return;
      const add = (paint, alpha) => {
        if (paint === "none") return;
        if (paint.startsWith("url(")) { onPaintServer(); return; }
        const ink = parse(paint);
        if (ink && ink[3] * alpha > 0) inks.push({ shape, ink: [ink[0], ink[1], ink[2], ink[3] * alpha] });
      };
      if (s.strokeWidth !== "0px") add(s.stroke, Number(s.strokeOpacity));
      if (shape.tagName.toLowerCase() !== "line") add(s.fill, Number(s.fillOpacity));
    });
    return inks;
  };
  const inkOn = (ink, el, ground) => over([ink[0], ink[1], ink[2], ink[3] * opacityOf(el)], ground);

  // The ground under an element as every value it can read: a gradient contributes each of its stops, so the lowest
  // ratio across the result is the lower end. A picture has no value, so it is reported.
  const STOP = /rgba?\([^)]+\)|color\(srgb [^)]+\)/g;
  const groundsOf = (el, includeSelf = true) => {
    const layers = [];
    let picture = false;
    for (let e = includeSelf ? el : el.parentElement; e; e = e.parentElement) {
      const s = getComputedStyle(e);
      overlays(e).forEach((c) => layers.push([c]));
      if (s.backgroundImage !== "none") {
        if (s.backgroundImage.includes("url(")) picture = true;
        const stops = (s.backgroundImage.match(STOP) || []).map(parse).filter(Boolean);
        if (stops.length) layers.push(stops);
      }
      const c = parse(s.backgroundColor);
      if (c && c[3] > 0) { layers.push([c]); if (c[3] >= 1) break; }
    }
    let grounds = [[255, 255, 255, 1]];
    for (let i = layers.length - 1; i >= 0; i -= 1) {
      const next = new Map();
      for (const g of grounds) for (const layer of layers[i]) { const v = over(layer, g); next.set(hex(v), v); }
      grounds = [...next.values()];
    }
    return { grounds, picture };
  };
  const lowest = (ink, el, grounds) => grounds.reduce((best, ground) => {
    const got = inkOn(ink, el, ground);
    const rt = ratio(got, ground);
    return !best || rt < best.ratio ? { ratio: rt, got, ground } : best;
  }, null);

  const outsideGallery = (el) => el.closest("[data-gallery-page]") === null;
  const where = (el) => {
    const name = (e) => e.tagName.toLowerCase() + (typeof e.className === "string" && e.className.trim() ? "." + e.className.trim().split(/\s+/)[0] : "");
    return el.parentElement ? `${name(el.parentElement)} > ${name(el)}` : name(el);
  };

  // A selector row from the rule file, without its state.
  const alternatives = (selector) => {
    const out = [];
    let depth = 0, cur = "";
    for (const ch of selector) {
      if (ch === "(" || ch === "[") depth += 1;
      if (ch === ")" || ch === "]") depth -= 1;
      if (ch === "," && depth === 0) { out.push(cur.trim()); cur = ""; } else cur += ch;
    }
    out.push(cur.trim());
    return out;
  };
  const pseudoOf = (alt) => (/::(before|after|placeholder)$/.exec(alt) || [])[0] || null;
  const bare = (alt) => alt.replace(/::(before|after|placeholder)$/, "").replace(/:(hover|active|focus-visible|focus)/g, "");
  const pick = (alt) => [...document.querySelectorAll(bare(alt))].filter(outsideGallery);

  // The paint a row names on an element, and what it stands on.
  const paintOf = (row, el, pseudo) => {
    const cs = getComputedStyle(el, pseudo || undefined);
    const edgeInk = () => {
      if (row.id === "focus") return parse(el.tagName === "INPUT" ? cs.borderTopColor : cs.outlineColor);
      for (const side of ["Top", "Bottom", "Left", "Right"]) if (parseFloat(cs[`border${side}Width`]) > 0) return parse(cs[`border${side}Color`]);
      return null;
    };
    if (row.kind === "edge") return { ink: edgeInk(), ground: groundsOf(el, false) };
    let ink;
    if (row.kind === "glyph") {
      if (pseudo) ink = parse(cs.borderLeftColor);
      else {
        const svg = el.tagName.toLowerCase() === "svg" ? el : el.querySelector("svg");
        const inks = svg ? svgInks(svg, () => {}) : [];
        if (inks.length) return { ink: inks[0].ink, owner: inks[0].shape, ground: groundsOf(el) };
        ink = parse(cs.color);
      }
    } else if (row.kind === "mark" && !el.textContent.trim()) ink = parse(cs.backgroundColor);
    else ink = parse(cs.color);
    const ground = groundsOf(el);
    const own = pseudo ? parse(cs.backgroundColor) : null;
    if (own && own[3] > 0) ground.grounds = ground.grounds.map((g) => over(own, g));
    return { ink, ground };
  };

  const judgeRow = (row, el, pseudo, tokens, theme) => {
    const { ink, owner, ground } = paintOf(row, el, pseudo);
    if (!ink || ink[3] === 0) return null;
    const low = lowest(ink, owner || el, ground.grounds);
    const want = tokens[row.token] && tokens[row.token][theme] ? parse(tokens[row.token][theme]) : null;
    return {
      ratio: +low.ratio.toFixed(2), got: hex(low.got), ground: hex(low.ground), picture: ground.picture,
      inkOk: want ? near(low.got, over(want, low.ground)) : false,
      below: row.grade !== null && low.ratio < row.grade,
      at: where(el),
    };
  };

  // The rows of the given states against whatever the page shows now.
  const directText = (el) => [...el.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
  // A text row reads an element that shows text. A family row spans a whole drawing, so it reads the element holding the text.
  const showsText = (row, el, pseudo, family) => pseudo || el.tagName === "INPUT" || (family ? directText(el) : el.textContent.trim() !== "");
  const measureRows = (rows, allRows, family, tokens, theme) => rows.map((row) => {
    const res = { id: row.id, found: [], measured: 0, inkHits: 0, results: [] };
    for (const alt of alternatives(row.selector)) {
      const pseudo = pseudoOf(alt);
      const els = pick(alt);
      if (els.length) res.found.push(alt);
      for (const el of els) {
        if (!visible(el)) continue;
        if (row.kind === "text" && !showsText(row, el, pseudo, family.includes(row.id))) continue;
        // A graded row does not read the mark or an ornament inside it. Those have their own rows.
        if (row.grade !== null && el.closest("[data-audit-exempt], [data-ink-grade]")) continue;
        const r = judgeRow(row, el, pseudo, tokens, theme);
        if (!r) continue;
        res.measured += 1;
        if (r.inkOk) res.inkHits += 1;
        if (r.below || !r.inkOk) {
          // Another row of the same kind may name this element: .site-nav a also matches the current link.
          r.otherRow = allRows.some((o) => o !== row && alternatives(o.selector).some((a) => el.matches(bare(a)) && (judgeRow(o, el, pseudoOf(a), tokens, theme) || {}).inkOk));
          res.results.push(r);
        }
      }
    }
    return res;
  });

  // Everything on screen outside a gallery specimen, whether a row names it or not.
  const scan = (scope, { exemptIds, noGradeInks, grades }) => {
    const out = { text: 0, glyph: 0, inputs: 0, svgs: 0, pseudo: 0, below: [], exemptUses: {}, badExempt: [], gradeNone: {}, picture: [], paintServer: 0 };
    const exemptOf = (el) => { const e = el.closest("[data-audit-exempt]"); return e ? e.getAttribute("data-audit-exempt") : null; };
    scope.querySelectorAll("[data-audit-exempt]").forEach((e) => {
      if (!outsideGallery(e)) return;
      const id = e.getAttribute("data-audit-exempt");
      if (!exemptIds.includes(id)) out.badExempt.push({ id, at: where(e) });
      out.exemptUses[id] = (out.exemptUses[id] || 0) + 1;
    });
    const textGrade = (el) => {
      const s = getComputedStyle(el);
      const px = parseFloat(s.fontSize);
      const bold = Number(s.fontWeight) >= 700;
      return px >= grades.displayMinPx || (bold && px >= grades.displayBoldMinPx) ? grades.display : grades.text;
    };
    const judge = (kind, el, ink, grade, text, pseudo) => {
      if (kind === "text") out.text += 1; else if (kind === "glyph") out.glyph += 1; else out.inputs += 1;
      if (pseudo) out.pseudo += 1;
      const { grounds, picture } = groundsOf(el);
      const own = pseudo ? parse(getComputedStyle(el, pseudo).backgroundColor) : null;
      const used = own && own[3] > 0 ? grounds.map((g) => over(own, g)) : grounds;
      const low = lowest(ink, el, used);
      const marked = el.closest("[data-ink-grade]");
      const native = el.closest(":disabled, [aria-disabled='true']");
      if (marked || native) {
        const why = marked ? marked.getAttribute("data-ink-why") : "disabled";
        // Disabled frees the disabled tokens from a grade, not every ink under a disabled ancestor.
        const free = why !== "disabled" || noGradeInks.some((v) => { const c = parse(v); return c && near(over(c, low.ground), over(ink, low.ground)); });
        if (free) { out.gradeNone[why] = (out.gradeNone[why] || 0) + 1; return; }
      }
      if (picture) { out.picture.push({ at: where(el), text }); return; }
      if (low.ratio < grade) out.below.push({ kind, ratio: +low.ratio.toFixed(2), grade, got: hex(low.got), ground: hex(low.ground), at: where(el), text });
    };
    const skip = (el) => !outsideGallery(el) || el.closest("script, style, noscript, template") !== null;
    const walker = document.createTreeWalker(scope, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const t = n.textContent.trim();
      if (!t) continue;
      const el = n.parentElement;
      if (el.closest("svg") || skip(el) || exemptOf(el) || !visible(el)) continue;
      const ink = parse(getComputedStyle(el).color);
      if (!ink || ink[3] === 0) continue;
      judge("text", el, ink, textGrade(el), t.slice(0, 40));
    }
    // Generated text such as a figure label has no text node.
    scope.querySelectorAll("*").forEach((el) => {
      if (el.closest("svg") || skip(el) || exemptOf(el) || !visible(el)) return;
      for (const pseudo of ["::before", "::after"]) {
        const cs = getComputedStyle(el, pseudo);
        if (cs.content === "none" || cs.content === "normal" || cs.content === '""' || cs.display === "none") continue;
        const ink = parse(cs.color);
        if (ink && ink[3] > 0) judge("text", el, ink, grades.text, cs.content.slice(0, 40), pseudo);
      }
    });
    scope.querySelectorAll("input:not([type=checkbox]):not([type=radio]), textarea").forEach((el) => {
      if (skip(el) || exemptOf(el) || !visible(el)) return;
      const empty = !el.value;
      if (empty && !el.placeholder) return;
      const ink = parse(getComputedStyle(el, empty ? "::placeholder" : undefined).color);
      if (ink && ink[3] > 0) judge("input", el, ink, grades.text, (el.value || el.placeholder).slice(0, 40));
    });
    scope.querySelectorAll("svg").forEach((el) => {
      if (skip(el) || exemptOf(el) || !visible(el)) return;
      out.svgs += 1;
      svgInks(el, () => { out.paintServer += 1; }).forEach(({ shape, ink }) => {
        const isText = shape.tagName.toLowerCase() === "text";
        judge(isText ? "text" : "glyph", shape, ink, isText ? grades.text : grades.nonText, isText ? shape.textContent.trim().slice(0, 40) : where(el));
      });
    });
    return out;
  };

  // For a pointer or press row: which element takes the pointer, and which is measured.
  const aim = (alt) => {
    const m = /^(.*?):(hover|active)(.*)$/.exec(bare.length ? alt : alt);
    const head = m ? m[1] : bare(alt);
    const tail = m ? m[3] : "";
    const candidates = [...document.querySelectorAll(head)].filter((e) => outsideGallery(e) && (m ? visible(e) : true));
    const el = candidates.find((e) => !e.matches("[aria-current]")) || candidates[0];
    if (!el) return null;
    const pseudo = pseudoOf(alt);
    const part = tail.replace(/::(before|after|placeholder)$/, "").trim();
    const target = part ? el.querySelector(part) : el;
    if (!target) return null;
    const pointer = m ? el : el.parentElement;
    pointer.scrollIntoView({ block: "center", behavior: "instant" });
    const r = pointer.getBoundingClientRect();
    window.__aimed = { pointer, target, pseudo };
    // With no pointer state in the selector the row is shown by the box around it, so aim at that box's corner.
    return m ? { x: r.left + r.width / 2, y: r.top + r.height / 2 } : { x: r.left + 2, y: r.top + 2 };
  };
  const measureAimed = (row, tokens, theme) => {
    const { pointer, target, pseudo } = window.__aimed;
    const reached = pointer.matches(":hover");
    if (!reached || !visible(target)) return { reached, measured: false };
    const r = judgeRow(row, target, pseudo, tokens, theme);
    return r ? { reached, measured: true, ...r } : { reached, measured: false };
  };
  const measureFocus = (row, tokens, theme) => {
    const el = document.activeElement;
    if (!el || el === document.body || !outsideGallery(el) || !el.matches(row.selector)) return null;
    return judgeRow(row, el, null, tokens, theme);
  };

  return { parse, over, ratio, hex, near, overlays, opacityOf, visible, svgInks, inkOn, measureRows, scan, alternatives, pseudoOf, bare, pick, aim, measureAimed, measureFocus };
}

export const installPageLib = (tab) => tab.addScriptTag({ content: `window.__contrast = (${pageLib})();` });
