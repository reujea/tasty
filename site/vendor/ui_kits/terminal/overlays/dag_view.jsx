import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Kbd } from "../../../components/core/Kbd.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { ListCtrl } from "../../../components/data/ListCtrl.jsx";
import { StatusDot } from "../../../components/feedback/StatusDot.jsx";
import { Checkbox } from "../../../components/forms/Checkbox.jsx";
import { Input } from "../../../components/forms/Input.jsx";
import { Select } from "../../../components/forms/Select.jsx";
import { DrillDown } from "../../../components/navigation/DrillDown.jsx";
import { Scrim } from "./shared.jsx";

// Tasty UI kit — Task DAG view.
// Two surfaces, ONE canvas: a full-tab graph surface (DagSurface) and a
// workspace-scoped popup that drills list → detail (DagWindow).
//
// READ-ONLY OBSERVATION, not a node editor: no ports/sockets, no drag-to-move,
// no connect affordance. Interaction is pan / zoom / select / switch DAG only.
//
// Resolved open decisions (§6 of the request):
//   1 direction  top-down default, with a direction toggle in the canvas chrome
//   2 routing    orthogonal elbows (4px corner radius)
//   3 minimap    yes, ≥560px surfaces only
//   4 LOD        3 tiers by zoom: full ≥0.7 · compact ≥0.4 · block <0.4
//   5 detail     right side panel (288) → bottom sheet (220) when narrow
//   6 running    STATIC only — accent wash + ◑ glyph + accent bar (0ms policy)
//   7 progress   "7/12" mono text
//   8 node time  duration on the meta row, full LOD only
//   9 background dot grid (16px)
//  10 dim        skipped/cancelled path (nodes AND their edges) at 0.4 opacity
//
// Layout is computed from the graph SHAPE only (ids + deps), never from status,
// so the 0.5s poll can repaint colours without moving a single node.



// ── State vocabulary ────────────────────────────────────────────────
// glyph = the non-colour channel (bare unicode, never emoji); label = the i18n
// string. Both ship with every status so colour is never the only signal.
const DAG_STATUS = {
  waiting:   { label: "Waiting",   glyph: "\u25E6" },
  ready:     { label: "Ready",     glyph: "\u276F" },
  running:   { label: "Running",   glyph: "\u25D1" },
  succeeded: { label: "Succeeded", glyph: "\u2713" },
  failed:    { label: "Failed",    glyph: "\u2717" },
  cancelled: { label: "Cancelled", glyph: "\u2212" },
  skipped:   { label: "Skipped",   glyph: "\u2298" },
  unknown:   { label: "Unknown",   glyph: "?" },
  // 2026-10-07 — ROLLUP-ONLY value (a DAG, never a task): no more progress, ≥1 unrecovered
  // failure, ≥1 branch succeeded to its end. Lower-half disc = "half the graph made it".
  partially_failed: { label: "Partially failed", glyph: "\u25D2" },
};
const DAG_STATUS_ORDER = ["waiting", "ready", "running", "succeeded", "failed", "cancelled", "skipped", "unknown"];
const DIM_STATUS = new Set(["skipped", "cancelled"]);
// 2026-10-07 — DAG rollup vocabulary + filter order (the app's 7 values). partially_failed sits
// between succeeded and failed: finished, outcome mixed.
const DAG_ROLLUP_ORDER = ["waiting", "ready", "running", "succeeded", "partially_failed", "failed", "skipped"];
// Rollup rule (mirrors the agent crate). Precedence: progress first, then outcome.
//  1 running → 2 ready → 3 waiting that can still run → 4 partially_failed | failed → 5 succeeded | skipped
//  · a failure is "recovered" when a fallback task for it succeeded
//  · end task = no downstream task; branch_not_selected skips don't count as downstream
//  · unknown is not progress (it blocks its dependents)
function dagRollup(nodes) {
  const by = Object.fromEntries(nodes.map((n) => [n.id, n]));
  const fallbackOf = (id) => nodes.filter((n) => (n.deps || []).some(([f, r]) => f === id && r === "fallback"));
  const recovered = (n) => fallbackOf(n.id).some((f) => f.status === "succeeded");
  const dead = (n) => (n.status === "failed" && !recovered(n)) || n.status === "cancelled" || n.status === "unknown"
    || (n.status === "skipped" && n.skip !== "branch_not_selected");
  const canRun = (n) => (n.deps || []).every(([f, r]) => r === "fallback" || !by[f] || !dead(by[f]));
  if (nodes.some((n) => n.status === "running")) return "running";
  if (nodes.some((n) => n.status === "ready")) return "ready";
  if (nodes.some((n) => n.status === "waiting" && canRun(n))) return "waiting";
  const downstream = (id) => nodes.some((n) => n.skip !== "branch_not_selected" && (n.deps || []).some(([f, r]) => f === id && r !== "fallback"));
  const failures = nodes.filter((n) => n.status === "failed" && !recovered(n));
  if (failures.length) {
    const endOk = nodes.some((n) => !downstream(n.id) && (n.status === "succeeded" || (n.status === "failed" && recovered(n))));
    return endOk ? "partially_failed" : "failed";
  }
  return nodes.some((n) => n.status === "succeeded") ? "succeeded" : "skipped";
}
// 2026-10-07 — running PHASE (typed v2 tasks only). executing = the default running look (null).
// awaiting_input is the one to notice: the graph is stopped on a person. postprocess / retry_wait
// stay in the running tone and only swap the label, carrying the run number.
const DAG_PHASE = {
  executing:      null,
  awaiting_input: { label: "Needs input", glyph: "!",       tone: "awaiting" },
  postprocessing: { label: "Postprocess", glyph: "\u25D1", tone: "running", showRun: true },
  retry_wait:     { label: "Retry wait",  glyph: "\u25D1", tone: "running", showRun: true },
};
// One look per node: status unless a phase overrides it.
function nodeLook(node) {
  const st = DAG_STATUS[node.status];
  const ph = node.status === "running" && node.phase ? DAG_PHASE[node.phase] : null;
  if (!ph) return { glyph: st.glyph, label: st.label, tone: node.status, tok: sTok(node.status), bg: sBg(node.status), lbl: sLabel(node.status) };
  const label = ph.showRun && node.run ? `${ph.label} \u00b7 run ${node.run}` : ph.label;
  const t = ph.tone === "awaiting"
    ? { tok: "var(--tasty-dag-phase-awaiting)", bg: "var(--tasty-dag-phase-awaiting-bg)", lbl: "var(--tasty-dag-phase-awaiting-label)" }
    : { tok: sTok("running"), bg: sBg("running"), lbl: sLabel("running") };
  return { glyph: ph.glyph, label, tone: ph.tone, ...t };
}
// Hover text: "name — label", then the why-line for skipped / unknown / awaiting.
const SKIP_WHY = { branch_not_selected: "Not selected by the upstream result", upstream_unavailable: "An upstream task did not succeed" };
function nodeTitle(node) {
  const lk = nodeLook(node);
  const lines = [`${node.name} \u2014 ${lk.label}`];
  if (node.status === "skipped" && node.skip) lines.push(`Why: ${SKIP_WHY[node.skip] || node.skip}`);
  if (node.status === "unknown" && node.reason) lines.push(`Why: ${node.reason}`, "Retry or cancel it to let the graph continue.");
  if (node.phase === "awaiting_input" && node.status === "running") lines.push(`Waiting for a person in the ${node.provider || "agent"} session${node.since ? ` \u00b7 ${node.since}` : ""}`);
  return lines.join("\n");
}
const DAG_KIND = {
  run:          { icon: "terminal", label: "run" },
  custom:       { icon: "plug",     label: "custom" },
  reduce:       { icon: "layers",   label: "reduce" },
  wait_barrier: { icon: "lock",     label: "barrier" },
  agent:        { icon: "agent",    label: "agent" },
};
const DAG_REL = {
  depends_on: { label: "depends on", color: "var(--tasty-dag-edge-depends)",  dash: null },
  fallback:   { label: "fallback",   color: "var(--tasty-dag-edge-fallback)", dash: "6 3" },
  reduce:     { label: "reduce",     color: "var(--tasty-dag-edge-reduce)",   dash: "2 3" },
  binding:    { label: "binds input", color: "var(--tasty-dag-edge-binding)", dash: "8 2 2 2" },   // 2026-10-07 — data edge; replaces depends_on on the same pair
  transition: { label: "transition",  color: "var(--tasty-dag-edge-transition)", dash: "10 4" },  // 2026-10-07 — result-chosen route; selection drawn per edge
};
const tk = (s) => String(s).replace(/_/g, "-");
const sTok = (s) => `var(--tasty-dag-status-${tk(s)})`;
const sBg = (s) => `var(--tasty-dag-status-${tk(s)}-bg)`;
// Text tone — readable at 10px even for the weak states (see components.css).
const sLabel = (s) => `var(--tasty-dag-status-${tk(s)}-label)`;

// ── Mock graphs ─────────────────────────────────────────────────────
const ERR_TAIL = `error: linking with \`cc\` failed: exit status: 1
  = note: /usr/bin/ld: cannot find -lssl: No such file or directory
          /usr/bin/ld: cannot find -lcrypto: No such file or directory
          collect2: error: ld returned 1 exit status

error: could not compile \`tasty-host\` (bin "tasty-host") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
make: *** [Makefile:42: release] Error 101`;

const DAG_BUILD = {
  id: "build-and-deploy",
  name: "build-and-deploy",
  workspace: "tasty",
  origin: "declared",
  updated: "12s ago",
  nodes: [
    { id: "checkout",     name: "checkout",            kind: "run",          status: "succeeded", dur: "1.2s",  started: "10:31:02", exit: 0, cmd: "git fetch --depth 1 && git checkout $SHA" },
    { id: "install",      name: "deps:install",        kind: "run",          status: "succeeded", dur: "24s",   started: "10:31:04", exit: 0, cmd: "cargo fetch --locked", deps: [["checkout", "depends_on"]] },
    { id: "lint",         name: "lint:clippy",         kind: "run",          status: "succeeded", dur: "8s",    started: "10:31:28", exit: 0, cmd: "cargo clippy -- -D warnings", deps: [["install", "depends_on"]] },
    { id: "unit",         name: "test:unit",           kind: "run",          status: "running",   dur: "12s",   started: "10:31:28", cmd: "cargo test --lib", deps: [["install", "depends_on"]] },
    { id: "e2e",          name: "test:e2e",            kind: "run",          status: "ready",     cmd: "cargo test --test e2e", deps: [["install", "depends_on"]] },
    { id: "build_linux",  name: "build:linux-x86_64",  kind: "run",          status: "failed",    dur: "41s",   started: "10:31:29", exit: 101, cmd: "cargo build --release --target x86_64-unknown-linux-gnu", err: ERR_TAIL, deps: [["install", "depends_on"]] },
    { id: "build_retry",  name: "build:linux (musl fallback)", kind: "run",  status: "succeeded", dur: "58s",   started: "10:32:11", exit: 0, cmd: "cargo build --release --target x86_64-unknown-linux-musl", deps: [["build_linux", "fallback"]] },
    { id: "build_mac",    name: "build:macos-arm64",   kind: "run",          status: "waiting",   cmd: "cargo build --release --target aarch64-apple-darwin", deps: [["unit", "depends_on"]] },
    { id: "sign",         name: "sign:artifacts",      kind: "custom",       status: "skipped",   cmd: "ipc: signer.sign(artifacts)", deps: [["build_linux", "depends_on"]] },
    { id: "package",      name: "package:release",     kind: "reduce",       status: "cancelled", cmd: "reduce: collect(build_retry, build_mac)", deps: [["build_retry", "reduce"], ["build_mac", "reduce"]] },
    { id: "gate",         name: "publish:gate",        kind: "wait_barrier", status: "waiting",   cmd: "barrier: await approval", deps: [["package", "depends_on"]] },
    { id: "notify",       name: "notify:agents",       kind: "custom",       status: "unknown",   cmd: "ipc: agents.broadcast(release)", deps: [["gate", "depends_on"]] },
  ],
  runner: { running: true, crashed: false, ready: 1, active: 1 },
};

const DAG_INDEX = {
  id: "index-refresh",
  name: "index-refresh",
  workspace: "tasty-docs",
  origin: "derived",
  updated: "2m ago",
  nodes: [
    { id: "scan",   name: "scan:sources",  kind: "run",    status: "succeeded", dur: "3s",  started: "10:22:40", exit: 0, cmd: "rg --files docs/" },
    { id: "a",      name: "chunk:a-m",     kind: "run",    status: "succeeded", dur: "11s", started: "10:22:43", exit: 0, cmd: "index chunk a-m", deps: [["scan", "depends_on"]] },
    { id: "b",      name: "chunk:n-z",     kind: "run",    status: "running",   dur: "9s",  started: "10:22:43", cmd: "index chunk n-z", deps: [["scan", "depends_on"]] },
    { id: "merge",  name: "merge:index",   kind: "reduce", status: "waiting",   cmd: "reduce: merge(a, b)", deps: [["a", "reduce"], ["b", "reduce"]] },
    { id: "swap",   name: "swap:live",     kind: "custom", status: "waiting",   cmd: "ipc: index.swap()", deps: [["merge", "depends_on"]] },
  ],
  runner: { running: true, crashed: false, ready: 0, active: 1 },
};

// 2026-10-07 — partially failed: the docs branch reached its end, the site branch's
// postprocess ran out of retries with no fallback. dagRollup(DAG_PARTIAL.nodes) === "partially_failed".
const DAG_PARTIAL = {
  id: "docs-publish", name: "docs-publish", workspace: "tasty-docs", origin: "declared", updated: "8m ago",
  nodes: [
    { id: "fetch",  name: "fetch:sources", kind: "run",   status: "succeeded", dur: "2s",  started: "10:12:01", exit: 0, cmd: "git pull --ff-only" },
    { id: "docs",   name: "build:docs",    kind: "run",   status: "succeeded", dur: "18s", started: "10:12:03", exit: 0, cmd: "mdbook build", deps: [["fetch", "depends_on"]] },
    { id: "upload", name: "upload:docs",   kind: "custom", status: "succeeded", dur: "4s", started: "10:12:21", exit: 0, cmd: "ipc: storage.put(book/)", deps: [["docs", "depends_on"]] },
    { id: "site",   name: "build:site",    kind: "run",   status: "succeeded", dur: "31s", started: "10:12:03", exit: 0, cmd: "zola build", deps: [["fetch", "depends_on"]] },
    { id: "deploy", name: "deploy:site",   kind: "run",   status: "failed",    dur: "1m 2s", started: "10:12:34", exit: 1, cmd: "rsync -a public/ web:/srv", deps: [["site", "depends_on"]] },
  ],
  runner: { running: true, crashed: false, ready: 0, active: 0 },
};

// A cyclic graph — the runner rejects it, the surface still draws it.
const DAG_CYCLE = {
  id: "release-notes",
  name: "release-notes",
  workspace: "tasty",
  origin: "declared",
  updated: "just now",
  cycle: ["draft", "review", "revise"],
  nodes: [
    { id: "collect", name: "collect:commits", kind: "run",    status: "succeeded", dur: "2s", started: "09:58:10", exit: 0, cmd: "git log --oneline" },
    { id: "draft",   name: "draft:notes",     kind: "custom", status: "unknown",  cmd: "ipc: writer.draft()", deps: [["collect", "depends_on"], ["revise", "depends_on"]] },
    { id: "review",  name: "review:notes",    kind: "custom", status: "waiting",  cmd: "ipc: reviewer.review()", deps: [["draft", "depends_on"]] },
    { id: "revise",  name: "revise:notes",    kind: "custom", status: "waiting",  cmd: "ipc: writer.revise()", deps: [["review", "depends_on"]] },
  ],
  runner: { running: false, crashed: false, ready: 2, active: 0 },
};

// Dense graph for the LOD / 200-node case — generated so the specimen can't
// drift from the layout code.
const DAG_DENSE = (() => {
  const nodes = [{ id: "n0", name: "fan:root", kind: "run", status: "succeeded", dur: "1s", started: "10:00:00", exit: 0, cmd: "fan out" }];
  const pool = ["succeeded", "succeeded", "succeeded", "running", "ready", "waiting", "failed", "skipped", "cancelled", "unknown"];
  for (let layer = 1; layer <= 6; layer++) {
    for (let i = 0; i < 9; i++) {
      const id = `n${layer}_${i}`;
      const parent = layer === 1 ? "n0" : `n${layer - 1}_${(i + (i % 2)) % 9}`;
      nodes.push({ id, name: `task:${layer}-${i}`, kind: "run", status: pool[(layer * 3 + i) % pool.length],
        dur: `${layer + i}s`, started: "10:0" + layer + ":0" + i, exit: 0, cmd: `run step ${layer}.${i}`,
        deps: [[parent, "depends_on"]] });
    }
  }
  return { id: "wide-fanout", name: "wide-fanout", workspace: "tasty", origin: "derived", updated: "4s ago",
    nodes, runner: { running: true, crashed: false, ready: 6, active: 4 } };
})();

const DAG_LIST = [
  { dag: DAG_BUILD, done: 4, total: 12, rollup: "failed" },
  { dag: DAG_INDEX, done: 2, total: 5, rollup: "running", skipped: 2, notSelected: 2, awaiting: 1 },
  { dag: DAG_DENSE, done: 26, total: 55, rollup: "running" },
  { dag: DAG_CYCLE, done: 1, total: 4, rollup: "unknown" },
  { dag: { id: "nightly-bench", name: "nightly-bench", workspace: "tasty-bench", origin: "declared", updated: "1h ago", nodes: DAG_INDEX.nodes, runner: { running: false, crashed: false, ready: 0, active: 0 } }, done: 8, total: 8, rollup: "succeeded" },
  { dag: { id: "migrate-store", name: "migrate-store", workspace: "tasty-lab", origin: "declared", updated: "22m ago", nodes: DAG_INDEX.nodes, runner: { running: false, crashed: true, ready: 3, active: 0 } }, done: 3, total: 9, rollup: "cancelled" },
  { dag: DAG_PARTIAL, done: 5, total: 5, rollup: "partially_failed" },
];

// ── Layout — layered, deterministic, status-independent ──────────────
const NODE_W = 168, NODE_H = 48, LAYER_GAP = 32, SIB_GAP = 24;

function dagLayout(dag, dir) {
  const nodes = dag.nodes;
  const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));
  const layer = Object.fromEntries(nodes.map((n) => [n.id, 0]));
  // Longest-path layering, capped by node count so a CYCLE terminates.
  for (let pass = 0; pass < nodes.length; pass++) {
    let moved = false;
    for (const n of nodes) {
      for (const [from] of n.deps || []) {
        if (byId[from] && layer[n.id] <= layer[from]) { layer[n.id] = layer[from] + 1; moved = true; }
      }
    }
    if (!moved) break;
  }
  const rows = {};
  for (const n of nodes) (rows[layer[n.id]] = rows[layer[n.id]] || []).push(n);
  const keys = Object.keys(rows).map(Number).sort((a, b) => a - b);
  const td = dir !== "lr";
  const across = td ? NODE_W : NODE_H;
  const widest = Math.max(...keys.map((k) => rows[k].length));
  const span = widest * across + (widest - 1) * SIB_GAP;
  const pos = {};
  for (const k of keys) {
    const row = rows[k];
    const len = row.length * across + (row.length - 1) * SIB_GAP;
    const off = (span - len) / 2;
    row.forEach((n, i) => {
      const a = off + i * (across + SIB_GAP);
      const d = k * ((td ? NODE_H : NODE_W) + LAYER_GAP);
      pos[n.id] = td ? { x: a, y: d } : { x: d, y: a };
    });
  }
  const width = td ? span : (keys.length * NODE_W + (keys.length - 1) * LAYER_GAP);
  const height = td ? (keys.length * NODE_H + (keys.length - 1) * LAYER_GAP) : span;
  return { pos, layer, byId, width, height, layers: keys.length };
}

// Orthogonal elbow with rounded corners. Top-down: down → across → down.
function elbow(s, t, td, r) {
  if (td) {
    if (Math.abs(s.x - t.x) < 1) return `M${s.x},${s.y} L${t.x},${t.y}`;
    const m = (s.y + t.y) / 2, d = t.x > s.x ? 1 : -1;
    return `M${s.x},${s.y} L${s.x},${m - r} Q${s.x},${m} ${s.x + d * r},${m} L${t.x - d * r},${m} Q${t.x},${m} ${t.x},${m + r} L${t.x},${t.y}`;
  }
  if (Math.abs(s.y - t.y) < 1) return `M${s.x},${s.y} L${t.x},${t.y}`;
  const m = (s.x + t.x) / 2, d = t.y > s.y ? 1 : -1;
  return `M${s.x},${s.y} L${m - r},${s.y} Q${m},${s.y} ${m},${s.y + d * r} L${m},${t.y - d * r} Q${m},${t.y} ${m + r},${t.y} L${t.x},${t.y}`;
}

// ── Node card ───────────────────────────────────────────────────────
// lod: "full" (name + status label + duration) · "compact" (glyph + name) ·
// "block" (status fill only). The BOX never changes size between tiers.
function DagNode({ node, lod = "full", selected = false, dimmed = false, onSelect, style }) {
  const [hot, setHot] = React.useState(false);
  const st = nodeLook(node);
  const kind = DAG_KIND[node.kind] || DAG_KIND.run;
  const accent = st.tok;
  const tip = nodeTitle(node);
  const base = {
    position: "absolute", boxSizing: "border-box", textAlign: "left",
    width: "var(--tasty-dag-node-width)", height: "var(--tasty-dag-node-height)",
    display: "flex", alignItems: "stretch", gap: 0, padding: 0, cursor: "pointer",
    borderRadius: "var(--tasty-dag-node-radius)",
    border: `var(--tasty-border-width) solid ${node.status === "waiting" || node.status === "cancelled" || node.status === "skipped" ? "var(--tasty-dag-node-border)" : accent}`,
    background: st.bg,
    outline: selected ? `var(--tasty-dag-node-selected-ring-width) solid var(--tasty-dag-node-selected-ring)` : "none",
    outlineOffset: "var(--tasty-size-1)",
    opacity: dimmed ? "var(--tasty-dag-node-dim-opacity)" : 1,
    fontFamily: "var(--tasty-font-ui)", color: "var(--tasty-text-on-active)",
    ...style,
  };
  if (lod === "block") {
    return (
      <button type="button" aria-label={`${node.name} — ${st.label}`} title={tip}
        onClick={() => onSelect && onSelect(node.id)} onMouseEnter={() => setHot(true)} onMouseLeave={() => setHot(false)}
        style={{ ...base, background: `color-mix(in srgb, ${accent} 55%, var(--tasty-surface-raised))`, borderColor: accent }} />
    );
  }
  return (
    <button type="button" aria-label={`${node.name} — ${st.label} — ${kind.label}`} title={tip}
      onClick={() => onSelect && onSelect(node.id)} onMouseEnter={() => setHot(true)} onMouseLeave={() => setHot(false)}
      style={base}>
      {/* leading status bar — the third, position-based state channel */}
      <span style={{ flex: "none", width: "var(--tasty-dag-node-bar-width)", background: accent,
        borderTopLeftRadius: "var(--tasty-radius-sm)", borderBottomLeftRadius: "var(--tasty-radius-sm)" }} />
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", justifyContent: "center",
        gap: "var(--tasty-dag-node-row-gap)", padding: "var(--tasty-dag-node-padding-y) var(--tasty-dag-node-padding-x)",
        background: hot ? "var(--tasty-dag-node-hover-bg)" : "transparent" }}>
        <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-dag-node-gap)", minWidth: 0 }}>
          <span style={{ flex: "none", display: "inline-flex", color: "var(--tasty-dag-node-meta-status-fg)" }}>
            <Icon name={kind.icon} size="var(--tasty-icon-size-sm)" />
          </span>
          <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            fontSize: "var(--tasty-dag-node-name-font-size)", fontWeight: "var(--tasty-font-weight-medium)" }}>
            {node.name}
          </span>
        </span>
        {lod === "full" && (
          <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-dag-node-gap)", minWidth: 0,
            fontSize: "var(--tasty-dag-node-meta-font-size)", letterSpacing: "var(--tasty-letter-spacing-caps)" }}>
            <span aria-hidden="true" style={{ flex: "none", color: st.lbl, fontFamily: "var(--tasty-font-mono)" }}>{st.glyph}</span>
            <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              textTransform: "uppercase", color: st.lbl }}>{st.label}</span>
            {node.dur && (
              <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", letterSpacing: "var(--tasty-letter-spacing-ui)",
                color: "var(--tasty-dag-node-meta-status-fg)" }}>{node.dur}</span>
            )}
          </span>
        )}
      </span>
    </button>
  );
}

// ── Runner badge ────────────────────────────────────────────────────
// The state to NOTICE: runner off with ready work — nobody is advancing the
// graph. It takes the warning tone and carries the resume hint.
function RunnerBadge({ runner, hint = true }) {
  const { running, crashed, ready, active } = runner;
  const stalled = !running && !crashed && ready > 0;
  const tone = crashed ? "crashed" : stalled ? "stalled" : null;
  const fg = crashed ? "var(--tasty-dag-runner-crashed-fg)" : stalled ? "var(--tasty-dag-runner-stalled-fg)"
    : running ? "var(--tasty-dag-runner-fg)" : "var(--tasty-dag-runner-idle-fg)";
  const text = crashed ? `Runner crashed \u00b7 ${ready} ready`
    : stalled ? `Runner stopped \u00b7 ${ready} ready`
    : running ? `Runner \u00b7 ${active} running \u00b7 ${ready} ready`
    : "Runner stopped \u00b7 no work";
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
      <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-dag-runner-gap)", flex: "none",
        height: "var(--tasty-dag-runner-height)", padding: "0 var(--tasty-dag-runner-padding-x)",
        borderRadius: "var(--tasty-dag-runner-radius)", boxSizing: "border-box",
        border: `var(--tasty-border-width) solid ${tone ? `var(--tasty-dag-runner-${tone}-border)` : "var(--tasty-dag-runner-border)"}`,
        background: tone ? `var(--tasty-dag-runner-${tone}-bg)` : "var(--tasty-dag-runner-bg)",
        fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: fg }}>
        <StatusDot status={crashed ? "error" : stalled ? "waiting" : running ? "running" : "idle"}
          pulse={running && !crashed && active > 0} />
        <span style={{ whiteSpace: "nowrap" }}>{text}</span>
      </span>
      {hint && (crashed || stalled) && (
        <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
          resume with <span style={{ fontFamily: "var(--tasty-font-mono)" }}>tasty dag runner start</span>
        </span>
      )}
    </span>
  );
}

// ── Canvas chrome ───────────────────────────────────────────────────
function ZoomCluster({ z, onZoom, onFit, dir, onDir, compact }) {
  const btn = (label, glyph, fn, key) => (
    <button key={key} type="button" aria-label={label} title={label} onClick={fn}
      style={{ appearance: "none", cursor: "pointer", display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: "var(--tasty-dag-chrome-height)", height: "var(--tasty-dag-chrome-height)", padding: 0, border: "none",
        background: "transparent", color: "var(--tasty-dag-chrome-fg)", fontFamily: "var(--tasty-font-mono)",
        fontSize: "var(--tasty-font-size-body)" }}>
      {typeof glyph === "string" ? glyph : glyph}
    </button>
  );
  return (
    <div style={{ display: "inline-flex", alignItems: "center", height: "var(--tasty-dag-chrome-height)",
      borderRadius: "var(--tasty-radius)", boxSizing: "border-box", overflow: "hidden",
      border: "var(--tasty-border-width) solid var(--tasty-dag-chrome-border)", background: "var(--tasty-dag-chrome-bg)" }}>
      {btn("Zoom out", "\u2212", () => onZoom(-1), "out")}
      {!compact && (
        <span style={{ minWidth: "var(--tasty-size-46)", textAlign: "center", fontFamily: "var(--tasty-font-mono)",
          fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-dag-chrome-fg)" }}>
          {Math.round(z * 100)}%
        </span>
      )}
      {btn("Zoom in", "+", () => onZoom(1), "in")}
      <span style={{ width: "var(--tasty-border-width)", alignSelf: "stretch", background: "var(--tasty-dag-chrome-border)" }} />
      {btn("Fit to view", <Icon name="move" size="var(--tasty-icon-size-sm)" />, onFit, "fit")}
      {btn(dir === "lr" ? "Flow top to bottom" : "Flow left to right", <Icon name="swap" size="var(--tasty-icon-size-sm)" />, onDir, "dir")}
    </div>
  );
}

function Minimap({ g, dag, z, ox, oy, vw, vh, dir }) {
  const pad = 4;
  const mw = 160, mh = 112;
  const k = Math.min((mw - pad * 2) / g.width, (mh - pad * 2) / g.height);
  const dx = (mw - g.width * k) / 2, dy = (mh - g.height * k) / 2;
  return (
    <div style={{ width: "var(--tasty-dag-minimap-width)", height: "var(--tasty-dag-minimap-height)", position: "relative",
      boxSizing: "border-box", borderRadius: "var(--tasty-radius)", overflow: "hidden",
      border: "var(--tasty-border-width) solid var(--tasty-dag-chrome-border)", background: "var(--tasty-dag-minimap-bg)" }}>
      <svg width={mw} height={mh} style={{ display: "block", position: "absolute", inset: 0 }} aria-hidden="true">
        {dag.nodes.map((n) => {
          const p = g.pos[n.id];
          return <rect key={n.id} x={dx + p.x * k} y={dy + p.y * k} width={Math.max(2, NODE_W * k)} height={Math.max(2, NODE_H * k)}
            fill={DIM_STATUS.has(n.status) ? "var(--tasty-dag-minimap-node-dim)" : sTok(n.status)} />;
        })}
        <rect x={dx + (-ox / z) * k} y={dy + (-oy / z) * k} width={(vw / z) * k} height={(vh / z) * k}
          fill="none" stroke="var(--tasty-dag-minimap-viewport)" strokeWidth="1" />
      </svg>
    </div>
  );
}

function CycleBanner({ ids }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", boxSizing: "border-box",
      minHeight: "var(--tasty-dag-cycle-height)", padding: "0 var(--tasty-space-md)",
      background: "var(--tasty-dag-cycle-bg)", color: "var(--tasty-dag-cycle-fg)",
      borderBottom: `var(--tasty-border-width) solid var(--tasty-dag-cycle-border)`,
      fontFamily: "var(--tasty-font-ui)", fontSize: "var(--tasty-font-size-caption)" }}>
      <span style={{ display: "inline-flex", flex: "none" }}><Icon name="alertTriangle" size="var(--tasty-icon-size-sm)" /></span>
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        Cycle detected — the runner will not advance {ids.length} tasks:{" "}
        <span style={{ fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>{ids.join(" \u2192 ")} \u2192 {ids[0]}</span>
      </span>
    </div>
  );
}

// ── Empty states ────────────────────────────────────────────────────
function DagEmpty({ variant = "surface", query = "" }) {
  // "popup" (2026-10-10 b12): the DAG list popup with no DAG in any workspace — no workspace-scoped command.
  // "filtered" (2026-10-12): no match with an empty/blank query — status filter or "This workspace only" emptied the list.
  const copy = variant === "search"
    ? { icon: "search", title: `No DAGs match “${query}”`, body: "Clear the filter or widen the scope to all workspaces." }
    : variant === "filtered"
    ? { icon: "filter", title: "No DAGs match the current filters", body: "Clear the filter or widen the scope to all workspaces." }
    : variant === "popup"
    ? { icon: "gitTree", title: "No DAGs yet", body: "Tasks grouped by dependency appear here." }
    : { icon: "gitTree", title: "No task DAGs in this workspace", body: "An agent creates one with tasty dag add; it appears here as soon as the host registers it." };
  return (
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xl)", textAlign: "center", fontFamily: "var(--tasty-font-ui)" }}>
      <span style={{ display: "inline-flex", color: "var(--tasty-text-disabled)" }}><Icon name={copy.icon} size="var(--tasty-size-24)" /></span>
      <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-secondary)" }}>{copy.title}</span>
      <span style={{ maxWidth: "var(--tasty-measure-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", textWrap: "pretty" }}>{copy.body}</span>
    </div>
  );
}

export { DAG_STATUS, DAG_STATUS_ORDER, DAG_ROLLUP_ORDER, DAG_PHASE, dagRollup, nodeLook, nodeTitle, DAG_KIND, DAG_REL, DIM_STATUS, sTok, sBg, sLabel, DAG_BUILD, DAG_INDEX, DAG_CYCLE, DAG_DENSE, DAG_PARTIAL, DAG_LIST, ERR_TAIL, dagLayout, elbow, DagNode, RunnerBadge, ZoomCluster, Minimap, CycleBanner, DagEmpty, NODE_W, NODE_H };
