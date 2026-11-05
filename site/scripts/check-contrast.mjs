/**
 * Renders the built site in Mocha and Latte and measures what is on screen: every gallery page against the contract, as
 * RULES.md section 14 and gallery/README.md "The check" in the Artifact say, and the site's own pages against
 * RULES.md section 15, in contrast-site.mjs.
 *
 *   npm run build && npm run check-contrast [-- --json <file>] [-- --page <key>] [-- --site-only] [-- --gallery-only]
 *
 * On a gallery page, fails on: a contract row with no frozen cell, a part that cannot be found, an ink that is not the row's
 * token, an ink below its grade, other text below 4.5, any painted shape of another glyph below 3, an unknown exempt id,
 * or nothing measured. An ink inside a disabled ancestor is measured at its grade unless it is a no-grade token.
 * Shapes composite through their own fill or stroke opacity and every opacity above them, a g inside the svg included.
 * --page names one gallery page and leaves the site pages out.
 * Browser: CHROMIUM_PATH, else the Playwright browser, else /snap/bin/chromium.
 */
import { createServer } from "node:http";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { BASE } from "../src/lib/base.mjs";
import { PART_MAP } from "../vendor/gallery/parts/part-map.js";
import { installPageLib } from "./contrast-page.mjs";
import { emptySiteTally, measureRest, runSite, siteRules } from "./contrast-site.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const args = process.argv.slice(2);
const argOf = (name) => (args.includes(name) ? args[args.indexOf(name) + 1] : null);
const THEMES = ["mocha", "latte"];
const EXPECTED_PAGES = 15;

const data = JSON.parse(readFileSync(join(root, "vendor/tokens/ground-ink-rules.json"), "utf8"));
const tokens = Object.fromEntries(data.tokens.map((t) => [t.name, t.values]));
const grades = Object.fromEntries(data.roles.map((r) => [r.id, { grade: r.grade, kind: r.kind }]));
const exemptIds = data.exceptions.filter((e) => e.audit === "subtree").map((e) => e.id);
// A token an ink may carry with no grade: its role has none, in some row or cell of the file.
const noGradeRoles = new Set(data.roles.filter((r) => r.grade === null).map((r) => r.id));
const noGradeTokens = new Set([...data.components, ...data.matrix].filter((r) => noGradeRoles.has(r.role) && r.token).map((r) => r.token));

function fail(message) {
  console.error(message);
  process.exit(2);
}

const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png" };
function serve() {
  const prefix = BASE.replace(/\/$/, "");
  const server = createServer((req, res) => {
    let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (prefix && path.startsWith(prefix)) path = path.slice(prefix.length);
    let file = join(dist, path);
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, "index.html");
    if (!existsSync(file)) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { "Content-Type": MIME[extname(file)] || "application/octet-stream" });
    res.end(readFileSync(file));
  });
  return new Promise((ok) => server.listen(0, "127.0.0.1", () => ok(server)));
}

function galleryPages() {
  const dir = join(dist, "design/gallery");
  if (!existsSync(dir)) fail("No built gallery. Run npm run build first.");
  const pages = [{ key: "foundations", path: "design/gallery/" }];
  for (const name of readdirSync(dir).sort()) {
    if (statSync(join(dir, name)).isDirectory()) pages.push({ key: name, path: `design/gallery/${name}/` });
  }
  return pages;
}

async function launch() {
  const candidates = [process.env.CHROMIUM_PATH, chromium.executablePath(), "/snap/bin/chromium"].filter(Boolean);
  const executablePath = candidates.find((p) => existsSync(p));
  if (!executablePath) fail(`No browser. Tried: ${candidates.join(", ")}. Set CHROMIUM_PATH.`);
  return chromium.launch({ executablePath });
}

// Runs in the page. Colors round per channel to 8 bits and a translucent layer composites source-over, as RULES.md says.
function measure({ rows, tokens, grades, partMap, exemptIds, noGradeTokens, theme }) {
  const { parse, over, ratio, hex, near, overlays, opacityOf, visible, inkOn } = window.__contrast;
  // The ground under an element: its own fill and overlays, then each ancestor's, down to the first opaque fill.
  const groundOf = (el, includeSelf = true) => {
    const stack = [];
    let picture = false;
    for (let e = includeSelf ? el : el.parentElement; e; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (s.backgroundImage !== "none") picture = true;
      overlays(e).forEach((c) => stack.push(c));
      const c = parse(s.backgroundColor);
      if (c && c[3] > 0) { stack.push(c); if (c[3] >= 1) break; }
    }
    let base = [255, 255, 255, 1];
    for (let i = stack.length - 1; i >= 0; i -= 1) base = over(stack[i], base);
    return { color: base, picture };
  };
  const out = { cells: 0, rows: [], text: 0, svgs: 0, emptySvgs: 0, glyph: 0, inputs: 0, below: [], exemptUses: {}, badExempt: [], gradeNone: [], picture: 0, paintServer: 0, keys: [] };
  const svgInks = (svg) => window.__contrast.svgInks(svg, () => { out.paintServer += 1; });

  const label = (el) => {
    const spec = el.closest("[data-spec]");
    return spec ? spec.getAttribute("data-spec") : "";
  };

  // Frozen cells
  const cells = Array.from(document.querySelectorAll("[data-gallery-page] [data-contract]"));
  out.cells = cells.length;
  cells.forEach((cell) => {
    const { contract, state, host } = cell.dataset;
    out.keys.push(contract + "|" + state + "|" + host);
    rows.filter((r) => r.component === contract && r.state === state && r.host === host).forEach((r) => {
      const map = (partMap[contract] && partMap[contract][r.part]) || {};
      const sel = map.sel || `[data-part="${r.part}"]`;
      const el = sel === ":cell" ? cell : cell.querySelector(sel);
      const want = tokens[r.token] && tokens[r.token][theme];
      const role = grades[r.role] || {};
      const base = { contract, state, host, part: r.part, token: r.token, role: r.role, grade: role.grade };
      if (!el) { out.rows.push({ ...base, status: "unlocated" }); return; }
      const cs = getComputedStyle(el, map.pseudo);
      let prop = map.prop;
      if (!prop) {
        if (r.part === "ring") prop = "outlineColor";
        else if (role.kind === "edge") {
          const bw = parseFloat(cs.borderTopWidth) > 0 ? "borderTopColor" : parseFloat(cs.borderBottomWidth) > 0 ? "borderBottomColor" : parseFloat(cs.borderLeftWidth) > 0 ? "borderLeftColor" : parseFloat(cs.borderRightWidth) > 0 ? "borderRightColor" : "backgroundColor";
          prop = bw;
        } else if (el.tagName.toLowerCase() === "svg" || el.querySelector("svg")) prop = "svg";
        else prop = "color";
      }
      if (r.part === "ring" && prop === "outlineColor" && cs.outlineStyle === "none") { out.rows.push({ ...base, status: "mismatch", got: "none", expect: want }); return; }
      const shadow = () => { const m = /(rgba?\([^)]+\)|color\([^)]+\))/.exec(cs.boxShadow); return m ? parse(m[1]) : null; };
      const glyphInks = prop === "svg" ? svgInks(el.tagName.toLowerCase() === "svg" ? el : el.querySelector("svg")) : [];
      const raw = prop === "svg" ? glyphInks[0]?.ink : prop === "shadow" ? shadow() : parse(cs[prop]);
      // An edge, a ring or a filled part stands on what is under it, not on itself.
      const ownFill = /^(border|outline|shadow|backgroundColor)/.test(prop);
      const ground = groundOf(ownFill ? el.parentElement : el).color;
      if (!raw || raw[3] === 0) { out.rows.push({ ...base, status: "mismatch", got: "none", expect: want, ground: hex(ground) }); return; }
      const got = inkOn(raw, prop === "svg" ? glyphInks[0].shape : el, ground);
      const exp = want && parse(want) ? over(parse(want), ground) : null;
      const rt = ratio(got, ground);
      const row = { ...base, got: hex(got), expect: want, ground: hex(ground), ratio: +rt.toFixed(2) };
      if (exp && !near(got, exp)) row.status = "mismatch";
      else if (role.grade && (rt < role.grade || glyphInks.some((g) => ratio(inkOn(g.ink, g.shape, ground), ground) < role.grade))) row.status = "below";
      else row.status = "ok";
      out.rows.push(row);
    });
  });

  // Everything else on the page
  const scope = document.querySelector("[data-gallery-page]");
  const skip = (el) => el.closest("[data-contract]") !== null;
  const exemptOf = (el) => {
    const e = el.closest("[data-audit-exempt]");
    return e ? e.getAttribute("data-audit-exempt") : null;
  };
  scope.querySelectorAll("[data-audit-exempt]").forEach((e) => {
    const id = e.getAttribute("data-audit-exempt");
    if (!exemptIds.includes(id)) out.badExempt.push({ id, spec: label(e) });
    out.exemptUses[id] = (out.exemptUses[id] || 0) + 1;
  });
  // A specimen can pin the other theme on its own box, so the no-grade values follow the nearest data-theme.
  const noGradeValues = (el) => {
    const scoped = el.closest("[data-theme]");
    const name = scoped && scoped !== document.documentElement ? (scoped.getAttribute("data-theme") === "latte" ? "latte" : "mocha") : theme;
    return noGradeTokens.map((t) => tokens[t] && tokens[t][name]).filter(Boolean).map((v) => parse(v)).filter(Boolean);
  };
  const judge = (kind, el, ink, grade, text) => {
    if (kind === "text") out.text += 1; else if (kind === "glyph") out.glyph += 1; else out.inputs += 1;
    const marked = el.closest("[data-ink-grade]");
    const native = el.closest(":disabled, [aria-disabled='true']");
    const { color: ground, picture } = groundOf(el);
    const got = inkOn(ink, el, ground);
    const rt = ratio(got, ground);
    if (marked || native) {
      const why = marked ? marked.getAttribute("data-ink-why") : "disabled";
      // Disabled frees the disabled tokens from a grade, not every ink under a disabled ancestor.
      const noGrade = why !== "disabled" || noGradeValues(el).some((v) => near(over(v, ground), over(ink, ground)));
      if (noGrade) { out.gradeNone.push({ why, spec: label(el) }); return; }
    }
    if (picture) { out.picture += 1; return; }
    if (rt < grade) out.below.push({ kind, ratio: +rt.toFixed(2), grade, got: hex(got), ground: hex(ground), spec: label(el), text });
  };
  const walker = document.createTreeWalker(scope, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const t = n.textContent.trim();
    if (!t) continue;
    const el = n.parentElement;
    if (el.closest("svg") || skip(el) || exemptOf(el) || !visible(el)) continue;
    const ink = parse(getComputedStyle(el).color);
    if (!ink || ink[3] === 0) continue;
    judge("text", el, ink, 4.5, t.slice(0, 40));
  }
  scope.querySelectorAll("input:not([type=checkbox]):not([type=radio]), textarea").forEach((el) => {
    if (skip(el) || exemptOf(el) || !visible(el)) return;
    const empty = !el.value;
    const ink = parse(getComputedStyle(el, empty && el.placeholder ? "::placeholder" : undefined).color);
    if (empty && !el.placeholder) return;
    if (ink) judge("input", el, ink, 4.5, (el.value || el.placeholder).slice(0, 40));
  });
  scope.querySelectorAll("svg").forEach((el) => {
    if (skip(el) || exemptOf(el) || !visible(el)) return;
    out.svgs += 1;
    const inks = svgInks(el);
    if (inks.length === 0) out.emptySvgs += 1;
    inks.forEach(({ shape, ink }) => {
      const isText = shape.tagName.toLowerCase() === "text";
      judge(isText ? "text" : "glyph", shape, ink, isText ? 4.5 : 3, isText ? shape.textContent.trim().slice(0, 40) : el.getAttribute("aria-label") || shape.getAttribute("class") || "svg");
    });
  });
  out.hscroll = document.documentElement.scrollWidth > document.documentElement.clientWidth;
  return out;
}

async function run() {
  const siteOnly = args.includes("--site-only");
  const galleryOnly = args.includes("--gallery-only") || argOf("--page") !== null;
  const pages = siteOnly ? [] : galleryPages().filter((p) => !argOf("--page") || p.key === argOf("--page"));
  if (!siteOnly && !argOf("--page") && pages.length !== EXPECTED_PAGES) fail(`Expected ${EXPECTED_PAGES} gallery pages, found ${pages.length}.`);
  const server = await serve();
  const port = server.address().port;
  const browser = await launch();
  const report = { themes: {} };
  let failed = false;
  const prefix = BASE.replace(/\/$/, "");
  const noGrade = [...noGradeTokens];
  const rules = siteRules(data);
  const origin = `http://127.0.0.1:${port}${prefix}`;
  for (const theme of THEMES) {
    const chrome = emptySiteTally(rules);
    if (!galleryOnly) {
      if (!(await runSite({ browser, origin, dist, rules, theme, report }))) failed = true;
    }
    if (siteOnly) continue;
    const seen = new Set();
    const t = { cells: 0, rows: 0, ok: 0, unlocated: [], mismatch: [], below: [], text: 0, svgs: 0, emptySvgs: 0, glyph: 0, inputs: 0, other: [], exemptUses: {}, badExempt: [], gradeNone: {}, picture: 0, paintServer: 0, hscroll: [], errors: [] };
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    await context.addInitScript((name) => localStorage.setItem("tasty-theme", name), theme);
    for (const page of pages) {
      const tab = await context.newPage();
      tab.on("pageerror", (e) => t.errors.push(`${page.key}: ${String(e).slice(0, 200)}`));
      await tab.goto(`${origin}/${page.path}?audit=1`, { waitUntil: "load", timeout: 180000 });
      await tab.waitForLoadState("networkidle", { timeout: 180000 }).catch(() => {});
      // Hydration, then stages and fetched tables. Measure once the page stops growing.
      let last = -1;
      for (let i = 0; i < 40; i += 1) {
        await tab.waitForTimeout(500);
        const size = await tab.evaluate(() => document.querySelectorAll("[data-gallery-page] *").length);
        if (size === last && !(await tab.evaluate(() => document.querySelector("astro-island[ssr]") !== null))) break;
        last = size;
      }
      await installPageLib(tab);
      const m = await tab.evaluate(measure, { rows: data.components, tokens, grades, partMap: PART_MAP, exemptIds, noGradeTokens: noGrade, theme });
      m.keys.forEach((k) => seen.add(k));
      t.cells += m.cells;
      t.rows += m.rows.length;
      for (const r of m.rows) {
        if (r.status === "ok") t.ok += 1;
        else t[r.status].push({ ...r, page: page.key });
      }
      t.text += m.text; t.svgs += m.svgs; t.emptySvgs += m.emptySvgs; t.glyph += m.glyph; t.inputs += m.inputs; t.picture += m.picture; t.paintServer += m.paintServer;
      m.below.forEach((b) => t.other.push({ ...b, page: page.key }));
      for (const [id, n] of Object.entries(m.exemptUses)) t.exemptUses[id] = (t.exemptUses[id] || 0) + n;
      m.badExempt.forEach((b) => t.badExempt.push({ ...b, page: page.key }));
      m.gradeNone.forEach((g) => { t.gradeNone[g.why] = (t.gradeNone[g.why] || 0) + 1; });
      if (m.hscroll) t.hscroll.push(page.key);
      await measureRest(tab, chrome, rules, theme, page.path, "gallery chrome");
      await tab.close();
    }
    await context.close();
    // The chrome around the specimens is the same layout, measured here on the pages the site pass leaves out.
    const chromeBad = chrome.below.length + chrome.badExempt.length + chrome.picture.length;
    console.log(`${theme} gallery chrome: pages ${chrome.pages}, text ${chrome.text}, shapes ${chrome.glyph}, below ${chrome.below.length}`);
    chrome.below.slice(0, 100).forEach((b) => console.log(`  chrome ${b.page} ${b.at}: ${b.kind} "${b.text}" ${b.got} on ${b.ground} ${b.ratio} < ${b.grade}`));
    if (chromeBad > 0) failed = true;
    const missing = [...new Set(data.components.map((r) => `${r.component}|${r.state}|${r.host}`))].filter((k) => !seen.has(k));
    t.missingCells = argOf("--page") ? [] : missing;
    report.themes[theme] = t;
    const measured = t.rows + t.text + t.glyph + t.inputs;
    const bad = t.unlocated.length + t.mismatch.length + t.below.length + t.other.length + t.badExempt.length + t.missingCells.length + t.errors.length;
    console.log(`${theme}: cells ${t.cells}, rows ${t.rows} (ok ${t.ok}, unlocated ${t.unlocated.length}, mismatch ${t.mismatch.length}, below ${t.below.length}), text ${t.text}, svgs ${t.svgs} (${t.glyph} painted shapes, ${t.emptySvgs} svgs painting nothing, ${t.paintServer} gradient or pattern paints not measured), inputs ${t.inputs}, below on page ${t.other.length}, on a picture ${t.picture}, missing cells ${t.missingCells.length}, page errors ${t.errors.length}`);
    console.log(`  exempt: ${Object.entries(t.exemptUses).map(([k, v]) => `${k} ${v}`).join(", ") || "none"}`);
    console.log(`  no grade: ${Object.entries(t.gradeNone).map(([k, v]) => `${k} ${v}`).join(", ") || "none"}`);
    if (measured === 0) { console.error(`${theme}: nothing measured`); failed = true; }
    if (bad > 0) failed = true;
    const pairs = new Map();
    for (const b of [...t.mismatch, ...t.below]) pairs.set(`${b.contract} ${b.part}: got ${b.got} want ${b.token} (${b.expect}) on ${b.ground}`, b);
    for (const b of t.other) pairs.set(`${b.page}#${b.spec}: ${b.kind} ${b.text} ${b.got} on ${b.ground} ${b.ratio} < ${b.grade}`, b);
    [...pairs.keys()].slice(0, 400).forEach((p) => console.log(`  ${p}`));
    t.unlocated.slice(0, 200).forEach((b) => console.log(`  unlocated ${b.contract} ${b.part}`));
    t.badExempt.forEach((b) => console.log(`  unknown exempt id ${b.id} at ${b.page}#${b.spec}`));
    t.missingCells.slice(0, 500).forEach((k) => console.log(`  no cell ${k}`));
    t.errors.forEach((e) => console.log(`  page error ${e}`));
  }
  await browser.close();
  server.close();
  if (argOf("--json")) writeFileSync(argOf("--json"), JSON.stringify(report, null, 1));
  console.log(failed ? "check-contrast: FAILED" : "check-contrast: passed");
  process.exit(failed ? 1 : 0);
}

run();
