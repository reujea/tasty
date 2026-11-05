/**
 * The site's own pages against site.places in tokens/ground-ink-rules.json (RULES.md section 15).
 * Every built page outside the gallery, in the given theme, wide and narrow, then pointer, press, focus and search passes.
 *
 * A row fails when its selector finds nothing on any page, when it measures no element, when an element is below its
 * grade, or when an element shows an ink no matching row names. Text no row names is still measured at 4.5, or 3 at
 * display size, and a shape at 3. A gradient is read at every stop. A family row is one selector shared by several inks,
 * so it only has to show its ink once, and every element under it is still judged at its grade.
 */
import { existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { installPageLib } from "./contrast-page.mjs";

const VIEWPORTS = [{ name: "wide", width: 1440, height: 1000 }, { name: "narrow", width: 390, height: 844 }];
const CONCURRENT_TABS = 6;

// Rows whose markup no built page has today: it depends on the release, on translation state or on what the guide text
// happens to use. The fixture is the markup those rules style, so a broken rule shows as a wrong ink. Everything else needs
// a real element. Each entry names the rows it stands in for, and the summary prints them.
const FIXTURES = [
  { page: "guide/getting-started/first-look/", at: ".content", rows: ["note-warning", "note-primary"],
    html: `<div class="note"><p>text</p><a href="#content">link</a></div>
<div class="note note--untranslated"><p>text</p></div>` },
  { page: "guide/getting-started/first-look/", at: ".prose", rows: ["prose-minor", "inline-code-link", "quote-link"],
    html: `<h4>Minor</h4><h5>Minor</h5><h6>Minor</h6>
<p><a href="#content"><code>code</code></a></p><blockquote><p>Quote <a href="#content">link</a></p></blockquote>` },
  { page: "", at: ".brand", rows: ["header-version"], html: `<span class="brand__version">v0.0.0</span>` },
];
export const FIXTURE_ROWS = FIXTURES.flatMap((f) => f.rows);
const injectFixtures = (tab, page) => Promise.all(FIXTURES.filter((f) => f.page === page).map((f) => tab.evaluate(
  ({ at, html }) => document.querySelector(at).insertAdjacentHTML("beforeend", html), { at: f.at, html: f.html })));
const CARDS = "design/cards/";
// The landing window starts on its third tab. The other two hold the rest of the drawn lines.
const LANDING_PAGES = ["", "ko/"];
const LANDING_TABS = ["claude · release", "README.md"];
const FOCUS_PAGES = ["", "guide/getting-started/first-look/"];
const SEARCH_PAGE = "guide/";
const TAB_STOPS = 14;

export function sitePages(dist) {
  const pages = [];
  const walk = (dir, rel) => {
    for (const name of readdirSync(dir).sort()) {
      const path = join(dir, name);
      const next = rel ? `${rel}/${name}` : name;
      if (statSync(path).isDirectory()) {
        if (next !== "design/gallery" && next !== "_astro") walk(path, next);
      } else if (name === "index.html") pages.push(rel ? `${rel}/` : "");
      else if (rel === "design/cards" && name.endsWith(".html")) pages.push(next);
    }
  };
  if (existsSync(dist)) walk(dist, "");
  return pages;
}

export function siteRules(data) {
  const rows = data.site.places;
  const bySelector = new Map();
  rows.forEach((r) => bySelector.set(`${r.kind} ${r.selector}`, (bySelector.get(`${r.kind} ${r.selector}`) || 0) + 1));
  const family = new Set(rows.filter((r) => bySelector.get(`${r.kind} ${r.selector}`) > 1 || r.selector === "a" || r.selector.startsWith(".shell ")).map((r) => r.id));
  const grades = data.site.grades;
  const tokens = Object.fromEntries(data.tokens.map((t) => [t.name, t.values]));
  const exemptIds = data.exceptions.filter((e) => e.audit === "subtree").map((e) => e.id);
  const noGradeRoles = new Set(data.roles.filter((r) => r.grade === null).map((r) => r.id));
  const noGradeNames = [...new Set([...data.components, ...data.matrix].filter((r) => noGradeRoles.has(r.role) && r.token).map((r) => r.token))];
  // The values, per theme: a page compares paint, and another theme's disabled value is an ordinary ink here.
  const noGrade = (theme) => noGradeNames.map((name) => tokens[name] && tokens[name][theme]).filter(Boolean);
  return { rows, family, grades, tokens, exemptIds, noGrade };
}

export function emptySiteTally(rules) {
  return {
    pages: 0, text: 0, glyph: 0, inputs: 0, svgs: 0, pseudo: 0, paintServer: 0, below: [], picture: [], badExempt: [], exemptUses: {}, gradeNone: {},
    rows: Object.fromEntries(rules.rows.map((r) => [r.id, { found: 0, measured: 0, inkHits: 0, below: [], mismatch: [], notes: [] }])),
    altsFound: {}, errors: [],
  };
}

function mergeScan(t, s, page, viewport) {
  t.text += s.text; t.glyph += s.glyph; t.inputs += s.inputs; t.svgs += s.svgs; t.pseudo += s.pseudo; t.paintServer += s.paintServer;
  s.below.forEach((b) => t.below.push({ ...b, page, viewport }));
  s.picture.forEach((b) => t.picture.push({ ...b, page, viewport }));
  s.badExempt.forEach((b) => t.badExempt.push({ ...b, page }));
  for (const [id, n] of Object.entries(s.exemptUses)) t.exemptUses[id] = (t.exemptUses[id] || 0) + n;
  for (const [why, n] of Object.entries(s.gradeNone)) t.gradeNone[why] = (t.gradeNone[why] || 0) + n;
}

function mergeRow(t, rules, res, page, viewport) {
  const row = t.rows[res.id];
  row.found += res.found.length;
  row.measured += res.measured;
  row.inkHits += res.inkHits;
  res.results.forEach((r) => {
    const rec = { ...r, page, viewport };
    if (r.below) row.below.push(rec);
    if (!r.inkOk && !r.otherRow && !rules.family.has(res.id)) row.mismatch.push(rec);
    if (r.picture) t.picture.push({ at: r.at, text: res.id, page, viewport });
  });
}

const needsScanArgs = (rules, theme) => ({ exemptIds: rules.exemptIds, noGradeInks: rules.noGrade(theme), grades: rules.grades });
const restRows = (rules) => rules.rows.filter((r) => r.state === "rest" || r.state === "selected");

// Measures the rest state of what a loaded tab shows. A gallery page comes here too, for the chrome around its specimens.
export async function measureRest(tab, t, rules, theme, page, viewport) {
  await installPageLib(tab);
  // A guideline document is a page of its own markup, whose class names can collide with a row. It has no rows.
  const rows = page.startsWith(CARDS) ? [] : restRows(rules);
  const args = { rows, allRows: rules.rows, family: [...rules.family], tokens: rules.tokens, theme, scanArgs: needsScanArgs(rules, theme) };
  const m = await tab.evaluate(({ rows: list, allRows, family, tokens, theme: name, scanArgs }) => {
    const lib = window.__contrast;
    return { rows: lib.measureRows(list, allRows, family, tokens, name), scan: lib.scan(document.body, scanArgs) };
  }, args);
  t.pages += 1;
  mergeScan(t, m.scan, page, viewport);
  m.rows.forEach((res) => mergeRow(t, rules, res, page, viewport));
}

async function settle(tab) {
  await tab.waitForLoadState("networkidle", { timeout: 60000 }).catch(() => {});
  let last = -1;
  for (let i = 0; i < 40; i += 1) {
    await tab.waitForTimeout(150);
    const size = await tab.evaluate(() => document.querySelectorAll("*").length);
    if (size === last && !(await tab.evaluate(() => document.querySelector("astro-island[ssr]") !== null))) break;
    last = size;
  }
}

const urlOf = (origin, page) => `${origin}/${page}`;

// Records which page first shows each pointer row's element, since most pages do not have all of them.
async function recordAlts(tab, t, rules, page) {
  await installPageLib(tab);
  const rows = rules.rows.filter((r) => r.state === "hover" || r.state === "press");
  const found = await tab.evaluate((list) => {
    const lib = window.__contrast;
    return list.flatMap((row) => lib.alternatives(row.selector).filter((alt) => lib.pick(alt).some((e) => lib.visible(e) || e.parentElement && lib.visible(e.parentElement))).map((alt) => `${row.id}\n${alt}`));
  }, rows);
  found.forEach((key) => { if (!t.altsFound[key]) t.altsFound[key] = page; });
}

async function restPass(context, origin, pages, t, rules, theme, viewport) {
  const queue = [...pages];
  const worker = async () => {
    for (;;) {
      const page = queue.shift();
      if (page === undefined) return;
      const tab = await context.newPage();
      tab.on("pageerror", (e) => t.errors.push(`${page}: ${String(e).slice(0, 200)}`));
      try {
        await tab.goto(urlOf(origin, page), { waitUntil: "load", timeout: 120000 });
        await settle(tab);
        await injectFixtures(tab, page);
        await measureRest(tab, t, rules, theme, page, viewport);
        if (viewport === "wide" && !page.startsWith(CARDS)) await recordAlts(tab, t, rules, page);
        if (viewport === "wide" && LANDING_PAGES.includes(page)) {
          for (const title of LANDING_TABS) {
            await tab.getByText(title, { exact: true }).first().click();
            await tab.waitForTimeout(100);
            await measureRest(tab, t, rules, theme, page, `${viewport}, tab ${title}`);
          }
        }
      } catch (e) {
        t.errors.push(`${page}: ${String(e).slice(0, 200)}`);
      }
      await tab.close();
    }
  };
  await Promise.all(Array.from({ length: CONCURRENT_TABS }, worker));
}

async function pointerPass(context, origin, t, rules, theme) {
  const rows = rules.rows.filter((r) => r.state === "hover" || r.state === "press");
  const tab = await context.newPage();
  let at = null;
  for (const row of rows) {
    if (row.selector.includes(".search__results")) continue;
    const rec = t.rows[row.id];
    const alts = Object.keys(t.altsFound).filter((k) => k.startsWith(`${row.id}\n`));
    for (const key of alts) {
      const alt = key.split("\n")[1];
      const page = t.altsFound[key];
      if (at !== page) {
        await tab.goto(urlOf(origin, page), { waitUntil: "load", timeout: 120000 });
        await settle(tab);
        await injectFixtures(tab, page);
        await installPageLib(tab);
        at = page;
      }
      const point = await tab.evaluate((a) => window.__contrast.aim(a), alt);
      if (!point) continue;
      rec.found += 1;
      await tab.mouse.move(point.x, point.y);
      if (row.state === "press") await tab.mouse.down();
      await tab.waitForTimeout(60);
      const r = await tab.evaluate(({ row: x, tokens, name }) => window.__contrast.measureAimed(x, tokens, name), { row, tokens: rules.tokens, name: theme });
      if (row.state === "press") { await tab.mouse.move(0, 0); await tab.mouse.up(); }
      if (!r.reached) rec.notes.push(`pointer did not reach ${alt} on ${page}`);
      if (!r.measured) continue;
      rec.measured += 1;
      if (r.inkOk) rec.inkHits += 1;
      if (r.below) rec.below.push({ ...r, page, viewport: row.state });
      if (!r.inkOk) rec.mismatch.push({ ...r, page, viewport: row.state });
    }
  }
  await tab.close();
}

async function focusPass(context, origin, t, rules, theme) {
  const row = rules.rows.find((r) => r.state === "focus");
  if (!row) return;
  const rec = t.rows[row.id];
  for (const page of FOCUS_PAGES) {
    const tab = await context.newPage();
    await tab.goto(urlOf(origin, page), { waitUntil: "load", timeout: 120000 });
    await settle(tab);
    await installPageLib(tab);
    for (let i = 0; i < TAB_STOPS; i += 1) {
      await tab.keyboard.press("Tab");
      // Reduced motion leaves a 0.01ms transition, so the first read would see the value before focus.
      await tab.waitForTimeout(30);
      const r = await tab.evaluate(({ row: x, tokens, name }) => window.__contrast.measureFocus(x, tokens, name), { row, tokens: rules.tokens, name: theme });
      if (!r) continue;
      rec.found += 1;
      rec.measured += 1;
      if (r.inkOk) rec.inkHits += 1;
      if (r.below) rec.below.push({ ...r, page, viewport: "focus" });
      if (!r.inkOk) rec.mismatch.push({ ...r, page, viewport: "focus" });
    }
    await tab.close();
  }
}

async function searchPass(context, origin, t, rules, theme) {
  const rows = rules.rows.filter((r) => r.selector.includes(".search__results"));
  const tab = await context.newPage();
  await tab.goto(urlOf(origin, SEARCH_PAGE), { waitUntil: "load", timeout: 120000 });
  await settle(tab);
  await installPageLib(tab);
  const measure = async (list) => {
    await tab.waitForTimeout(30);
    const m = await tab.evaluate(({ rows: x, allRows, family, tokens, name, scanArgs }) => {
      const lib = window.__contrast;
      return { rows: lib.measureRows(x, allRows, family, tokens, name), scan: lib.scan(document.querySelector(".search__results"), scanArgs) };
    }, { rows: list, allRows: rules.rows, family: [...rules.family], tokens: rules.tokens, name: theme, scanArgs: needsScanArgs(rules, theme) });
    mergeScan(t, m.scan, SEARCH_PAGE, "search");
    m.rows.forEach((res) => mergeRow(t, rules, res, SEARCH_PAGE, "search"));
  };
  await tab.click(".search input");
  await tab.fill(".search input", "tasty");
  await tab.waitForSelector(".search__results[data-open] a", { timeout: 20000 });
  await measure(rows.filter((r) => !r.selector.includes("[data-active]")));
  await tab.keyboard.press("ArrowDown");
  await tab.waitForSelector(".search__results a[data-active]", { timeout: 5000 });
  await measure(rows.filter((r) => r.selector.includes("[data-active]")));
  await tab.fill(".search input", "zzzzqqqq");
  await tab.waitForSelector(".search__empty", { timeout: 5000 });
  await measure([]);
  await tab.close();
}

export async function runSite({ browser, origin, dist, rules, theme, report }) {
  const pages = sitePages(dist);
  const t = emptySiteTally(rules);
  for (const vp of VIEWPORTS) {
    const context = await browser.newContext({ viewport: { width: vp.width, height: vp.height }, reducedMotion: "reduce" });
    await context.addInitScript((name) => localStorage.setItem("tasty-theme", name), theme);
    await restPass(context, origin, pages, t, rules, theme, vp.name);
    if (vp.name === "wide") {
      await pointerPass(context, origin, t, rules, theme);
      await focusPass(context, origin, t, rules, theme);
      await searchPass(context, origin, t, rules, theme);
    }
    await context.close();
  }
  return summarize(t, rules, theme, pages.length, report);
}

// Prints the tally and says whether this theme passed.
export function summarize(t, rules, theme, pageCount, report) {
  const unlocated = [], unmeasured = [], below = [], mismatch = [], notes = [];
  for (const row of rules.rows) {
    const r = t.rows[row.id];
    if (r.found === 0) unlocated.push(row.id);
    else if (r.measured === 0) unmeasured.push(row.id);
    r.below.forEach((b) => below.push(`${row.id} [${b.viewport}] ${b.page} ${b.at}: ${b.got} on ${b.ground} ${b.ratio} < ${row.grade}`));
    r.mismatch.forEach((b) => mismatch.push(`${row.id} [${b.viewport}] ${b.page} ${b.at}: ${b.got} on ${b.ground}, wants ${row.token}`));
    if (r.measured > 0 && r.inkHits === 0) mismatch.push(`${row.id}: no element shows ${row.token}`);
    r.notes.forEach((n) => notes.push(`${row.id}: ${n}`));
  }
  const bad = unlocated.length + unmeasured.length + below.length + mismatch.length + t.below.length + t.picture.length + t.badExempt.length + t.errors.length;
  const measured = t.text + t.glyph + t.inputs;
  console.log(`${theme} site: pages ${t.pages} of ${pageCount} (each wide and narrow), rows ${rules.rows.length} (unlocated ${unlocated.length}, nothing measured ${unmeasured.length}, below ${below.length}, wrong ink ${mismatch.length}), text ${t.text} (generated ${t.pseudo}), shapes ${t.glyph} in ${t.svgs} svgs (${t.paintServer} gradient paints not measured), inputs ${t.inputs}, below on page ${t.below.length}, on a picture ${t.picture.length}, page errors ${t.errors.length}`);
  console.log(`  measured through a fixture, no built page has the markup: ${FIXTURE_ROWS.join(", ")}`);
  console.log(`  exempt: ${Object.entries(t.exemptUses).map(([k, v]) => `${k} ${v}`).join(", ") || "none"}`);
  console.log(`  no grade: ${Object.entries(t.gradeNone).map(([k, v]) => `${k} ${v}`).join(", ") || "none"}`);
  unlocated.forEach((id) => console.log(`  no element for row ${id}`));
  unmeasured.forEach((id) => console.log(`  nothing measured for row ${id}`));
  // A few lines per row, so one broad failure does not hide another row.
  const perRow = (lines, tag) => {
    const shown = new Map();
    new Set(lines).forEach((l) => {
      const id = l.split(/[ :]/)[0];
      shown.set(id, (shown.get(id) || 0) + 1);
      if (shown.get(id) <= 5) console.log(`  ${tag} ${l}`);
    });
  };
  perRow(below, "below");
  perRow(mismatch, "wrong ink");
  const seen = new Set();
  t.below.forEach((b) => {
    const key = `${b.at} ${b.text} ${b.got} ${b.ground}`;
    if (seen.has(key) || seen.size >= 300) return;
    seen.add(key);
    console.log(`  on page [${b.viewport}] ${b.page} ${b.at}: ${b.kind} "${b.text}" ${b.got} on ${b.ground} ${b.ratio} < ${b.grade}`);
  });
  t.picture.slice(0, 50).forEach((b) => console.log(`  on a picture ${b.page} ${b.at} ${b.text}`));
  t.badExempt.forEach((b) => console.log(`  unknown exempt id ${b.id} at ${b.page} ${b.at}`));
  [...new Set(notes)].forEach((n) => console.log(`  ${n}`));
  [...new Set(t.errors)].slice(0, 50).forEach((e) => console.log(`  page error ${e}`));
  if (measured === 0) console.error(`${theme} site: nothing measured`);
  report.site = report.site || {};
  report.site[theme] = { ...t, unlocated, unmeasured };
  return bad === 0 && measured > 0;
}
