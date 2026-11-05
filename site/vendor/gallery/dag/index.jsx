import React from "react";
import { PageStates } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Kbd, ListCtrl } from "../../components/index.js";
import { DAG_STATUS, DAG_STATUS_ORDER, DAG_KIND, DAG_REL, sTok, DagNode, RunnerBadge, ZoomCluster, Minimap, CycleBanner, DagEmpty, DagCanvas, DagDetail, DagSurface, DagWindow, dagRowItems, dagLayout, elbow, DAG_BUILD, DAG_INDEX, DAG_CYCLE, DAG_DENSE, DAG_LIST, DAG_PARTIAL, DAG_ROLLUP_ORDER, dagRollup, nodeTitle } from "../../ui_kits/terminal/dag.js";
import { Section, Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Surfaces · Task DAG
// Specimens for the read-only Task DAG surface + workspace popup
// Parts live in ui_kits/terminal/overlays/
// dag_view.jsx + dag_surfaces.jsx; this page only catalogues them.





const NAV = [
  { id: "canvas", label: "Graph canvas" },
  { id: "node", label: "Node card · 8 states" },
  { id: "phase", label: "Running phase · unknown reason" },
  { id: "edges", label: "Edges · 5 relations" },
  { id: "routes", label: "Transitions · not selected" },
  { id: "chrome", label: "Canvas chrome" },
  { id: "runner", label: "Runner badge" },
  { id: "rows", label: "DAG list row" },
  { id: "detail", label: "Node detail" },
  { id: "states", label: "Empty · cycle" },
  { id: "surfaces", label: "Surface · popup" },
];

// A node card outside the canvas: the card is absolutely positioned inside the
// graph layer, so specimens give it a 168 × 48 relative box.
function NodeBox({ node, lod = "full", selected = false, dimmed = false, caption }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ position: "relative", width: 168, height: 48 }}>
        <DagNode node={node} lod={lod} selected={selected} dimmed={dimmed} />
      </div>
      {caption && <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{caption}</span>}
    </div>
  );
}

const mk = (over) => ({ id: "spec", name: "build:linux-x86_64", kind: "run", status: "running", dur: "12s", cmd: "cargo build", ...over });

function EdgeSpecimen({ rel }) {
  const meta = DAG_REL[rel];
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6, alignItems: "center" }}>
      <svg width="120" height="56" viewBox="0 0 120 56" aria-hidden="true">
        <path d={elbow({ x: 12, y: 8 }, { x: 108, y: 48 }, true, 4)} fill="none" stroke={meta.color} strokeWidth="1" strokeDasharray={meta.dash || undefined} />
        <polygon points="104,41 112,41 108,48" fill={meta.color} />
      </svg>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{rel}</span>
      <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{meta.label}</span>
    </div>
  );
}

function PopupStage() {
  const [open, setOpen] = React.useState(true);
  return (
    <div style={{ position: "relative", width: "100%", height: 520, borderRadius: "var(--tasty-radius)", overflow: "hidden",
      border: "var(--tasty-border-width) solid var(--tasty-border-default)", background: "var(--tasty-bg-app)" }}>
      {!open && (
        <button type="button" onClick={() => setOpen(true)}
          style={{ position: "absolute", left: 12, top: 12, height: 28, padding: "0 12px", cursor: "pointer",
            borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
            background: "var(--tasty-surface-raised)", color: "var(--tasty-text-secondary)", fontFamily: "var(--tasty-font-ui)", fontSize: 13 }}>
          Reopen popup
        </button>
      )}
      {open && <DagWindow onClose={() => setOpen(false)} />}
    </div>
  );
}

function Page() {
  const [selected, setSelected] = React.useState("build_linux");
  return (
    <>
      <Section id="canvas" title="Graph canvas">
        <Spec id="task-dag-canvas-read-only-observation" title="Task DAG canvas — read-only observation"
          when={<>The one canvas both surfaces embed. Nodes are laid out <b>top-down by dependency layer</b>, edges are <b>orthogonal</b> with 4px elbows. This is <b>not a node editor</b>: there are no ports, no sockets, no drag-to-move — the only gestures are <b>pan</b> (drag empty canvas or middle-drag), <b>zoom</b> (<Kbd keys="Ctrl" /> + wheel; bare wheel pans, because a terminal app reads bare wheel as scroll), <b>select</b> (click a node), and <Kbd keys="Esc" /> to clear.</>}>
          <Stage style={{ padding: 0, height: 460, display: "block" }}>
            <div style={{ height: 460, display: "flex" }}>
              <DagCanvas dag={DAG_BUILD} dir="td" selected={selected} onSelect={setSelected} />
            </div>
          </Stage>
          <Meta
            specs={[["node box", "168 × 48 (fixed at every zoom)"], ["layer gap", "32 along flow · 24 across"], ["zoom", "20% – 150%, 10% steps"], ["fit", "auto on open, capped at 100%"], ["background", "dot grid · 16px"], ["poll", "0.5s — colour only, never re-layout"]]}
            tokens={[{ tok: "--tasty-dag-canvas-bg", use: "canvas bed", color: "var(--tasty-dag-canvas-bg)" }, { tok: "--tasty-dag-canvas-dot", use: "grid mark", color: "var(--tasty-dag-canvas-dot)" }, { tok: "--tasty-dag-layer-gap", use: "32 flow depth" }, { tok: "--tasty-dag-sibling-gap", use: "24 across" }, { tok: "--tasty-dag-edge-corner-radius", use: "elbow" }]} />
          <Note><b>Position stability</b> is the contract that makes a 0.5s poll survivable: layout is computed from ids + dependency edges only — never from status, duration or counts — so a task turning <code>running → succeeded</code> repaints one card and moves nothing. Layering is longest-path and capped by node count, so a cyclic graph still terminates and still draws.</Note>
          <Dont>Never add a connect handle, a port dot, or a drag-to-move affordance. Anything that looks editable promises an edit the host cannot honour — DAGs are built by agents over CLI/IPC.</Dont>
        </Spec>
      </Section>

      <Section id="node" title="Node card · 8 states">
        <Spec id="task-node-every-execution-state" title="Task node — every execution state"
          when={<>One card = one task. Three channels carry state so <b>colour is never alone</b>: the <b>leading 3px bar</b> (position), a <b>mono glyph</b> (◦ ❯ ◑ ✓ ✗ − ⊘ ?), and the <b>spelled label</b>. <code>running</code> is the loudest — accent border + accent wash — and it is <b>static</b>: the 0ms terminal-motion policy holds, so there is no spinner and no pulse on the card.</>}>
          <Stage variant="grid" style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 20, alignItems: "flex-start" }}>
            {DAG_STATUS_ORDER.map((s) => (
              <NodeBox key={s} node={mk({ status: s, name: `task:${s}`, dur: s === "waiting" || s === "ready" ? null : "12s" })}
                dimmed={s === "skipped" || s === "cancelled"} caption={s} />
            ))}
          </Stage>
          <Meta
            specs={[["card", "168 × 48 · radius 4"], ["padding", "4 / 8 · row gap 4"], ["status bar", "3px, full height"], ["name", "13 medium, 1 line, ellipsis"], ["meta row", "10 caps · duration mono, right"], ["dimmed", "skipped + cancelled at 0.4"]]}
            tokens={DAG_STATUS_ORDER.map((s) => ({ tok: `--tasty-dag-status-${s}`, use: DAG_STATUS[s].label.toLowerCase(), color: sTok(s) }))} />
          <Note><b>Colour vs. text:</b> the state colour is the <i>recede</i> channel — bar, border, glyph on a chip. The spelled label reads from its own <code>--tasty-dag-status-&lt;state&gt;-label</code> role: waiting takes <code>--tasty-text-muted</code>, and the two dimmed states take <code>--tasty-text-secondary</code>, which still measures 4.74:1 after the 0.75 card dim.</Note>
          <Note><b>Truncation rule (i18n):</b> the task name is a single line with ellipsis at the card edge — never wraps, never shrinks the font; the full name lives in the tooltip and the detail panel. On the meta row the <b>status label yields first</b>: it ellipses before the duration is dropped, so a long German or Japanese state string can't push the timing out of the card.</Note>
        </Spec>

        <Spec id="task-kinds" title="Task kinds"
          when={<>Five task kinds, told apart by the leading glyph: <code>run</code> (shell), <code>custom</code> (IPC call), <code>reduce</code> (result fan-in), <code>wait_barrier</code> (gate), <code>agent</code> (one Claude / Codex turn, 2026-10-07). Claude and Codex share the <code>agent</code> glyph; the provider is a detail-panel field, not a second shape.</>}>
          <Stage variant="grid" style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 20 }}>
            {Object.keys(DAG_KIND).map((k) => (
              <NodeBox key={k} node={mk({ kind: k, status: "succeeded", name: `${DAG_KIND[k].label}:step`, dur: "4s" })} caption={k} />
            ))}
          </Stage>
          <Meta specs={[["glyph", "14px, muted, leading"], ["run", "terminal"], ["custom", "plug"], ["reduce", "layers"], ["wait_barrier", "lock"], ["agent", "agent (new) · stroke = icon-stroke-width like the rest · detail kind label “Agent”"]]}
            tokens={[{ tok: "--tasty-dag-node-meta-fg", use: "kind glyph", color: "var(--tasty-dag-node-meta-fg)" }]} />
          <Note>New glyph: <code>icons/agent.svg</code> — a speech bubble carrying the prompt chevron (an agent turn is a conversation that runs commands). <code>terminal</code>, <code>plug</code>, <code>layers</code> and <code>lock</code> carry the other four. A dedicated <code>barrier</code> glyph would read better than <code>lock</code> if one is ever drawn.</Note>
        </Spec>

        <Spec id="level-of-detail-selection-overflow" title="Level of detail · selection · overflow"
          when={<>Zooming out never changes the <b>box</b>, only its <b>content</b>: <b>full</b> (≥70%) name + status + duration · <b>compact</b> (≥40%) glyph + name · <b>block</b> (&lt;40%) status fill only. At 200 nodes the block tier keeps every state readable as colour while the graph shape stays legible.</>}>
          <Stage variant="grid" style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 20, alignItems: "flex-start" }}>
            <NodeBox node={mk({ status: "running" })} lod="full" caption="full ≥ 70%" />
            <NodeBox node={mk({ status: "running" })} lod="compact" caption="compact ≥ 40%" />
            <NodeBox node={mk({ status: "running" })} lod="block" caption="block < 40%" />
            <NodeBox node={mk({ status: "failed", name: "build:linux-x86_64-with-a-very-long-target-triple" })} caption="overflow → ellipsis" />
            <NodeBox node={mk({ status: "succeeded" })} selected caption="selected (2px ring)" />
          </Stage>
          <Meta specs={[["full", "zoom ≥ 0.7"], ["compact", "0.4 – 0.7"], ["block", "< 0.4"], ["selection", "2px accent outline, 1px offset"], ["hover", "derived overlay-hover on the body"]]}
            tokens={[{ tok: "--tasty-dag-node-selected-ring", use: "selection", color: "var(--tasty-dag-node-selected-ring)" }, { tok: "--tasty-dag-node-hover-bg", use: "hover wash" }, { tok: "--tasty-dag-node-dim-opacity", use: "skipped path" }]} />
          <Stage style={{ padding: 0, height: 360, display: "block" }}>
            <div style={{ height: 360, display: "flex" }}><DagCanvas dag={DAG_DENSE} dir="td" /></div>
          </Stage>
          <Note>55 nodes, auto-fitted on open — the canvas lands in the <b>compact</b> or <b>block</b> tier and says so in the bottom-left hint chip, so a user who sees colour blocks knows why and knows the way back.</Note>
        </Spec>
      </Section>

      <Section id="phase" title="Running phase · unknown reason">
        <Spec id="running-phase-only-awaiting-input-gets-its-own-look" title="Running phase — only awaiting_input gets its own look"
          when={<>Typed (v2) tasks report a step while running. <code>executing</code> is the plain running card. <b><code>awaiting_input</code></b> stops the graph on a person, so it swaps the whole state look to the needs-input yellow — bar, border, wash, glyph <code>!</code> and label — and stays readable at the <b>compact</b> tier (glyph + name). <code>postprocessing</code> and <code>retry_wait</code> keep the running tone and only swap the label, with the run number.</>}>
          <Stage variant="grid" style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 20, alignItems: "flex-start" }}>
            <NodeBox node={mk({ phase: "executing", name: "test:unit" })} caption="executing (default)" />
            <NodeBox node={mk({ kind: "agent", phase: "awaiting_input", provider: "claude", since: "2m 4s", name: "agent:review", dur: "2m" })} caption="awaiting_input" />
            <NodeBox node={mk({ kind: "agent", phase: "awaiting_input", provider: "claude", name: "agent:review" })} lod="compact" caption="awaiting_input · compact" />
            <NodeBox node={mk({ phase: "postprocessing", run: 1, name: "build:docs", dur: "41s" })} caption="postprocessing" />
            <NodeBox node={mk({ phase: "retry_wait", run: 2, name: "build:docs", dur: "1m 3s" })} caption="retry_wait" />
          </Stage>
          <Meta
            specs={[["executing", "no change — RUNNING"], ["awaiting_input", "! · NEEDS INPUT · yellow · duration = time waiting"], ["postprocessing", "◑ · POSTPROCESS · RUN n"], ["retry_wait", "◑ · RETRY WAIT · RUN n"], ["v1 tasks", "never have a phase → plain running"]]}
            tokens={[{ tok: "--tasty-dag-phase-awaiting", use: "bar · border", color: "var(--tasty-dag-phase-awaiting)" }, { tok: "--tasty-dag-phase-awaiting-bg", use: "card wash", color: "var(--tasty-dag-phase-awaiting-bg)" }, { tok: "--tasty-dag-phase-awaiting-label", use: "glyph + label", color: "var(--tasty-dag-phase-awaiting-label)" }]} />
          <Note><b>Labels (i18n).</b> needs input · 입력 대기 · 入力待ち — postprocess · 후처리 · 後処理 — retry wait · 재시도 대기 · 再試行待ち — run {"{n}"} · {"{n}"}회차 · {"{n}"}回目. The meta row is caps for Latin scripts only.</Note>
          <Note><b>Click.</b> Selecting an awaiting node opens the detail panel with a yellow notice and an <b>Open session</b> button (secondary sm) that focuses the agent session surface. It is a user action only; agents never trigger it. The DAG list row adds a mono <code>! 1 needs input</code> before the rollup chip; the rollup itself stays <b>running</b>.</Note>
        </Spec>
        <Spec id="hover-the-why-line-for-skipped-unknown-and-awaiting" title="Hover — the why-line for skipped, unknown and awaiting"
          when={<>The node tooltip is <code>name — label</code> plus one why-line. Unknown also says what unblocks it. The same text sits in the detail panel under <b>Why unknown</b>.</>}>
          <Stage variant="grid" style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 20, alignItems: "flex-start" }}>
            {[mk({ status: "unknown", name: "deploy:site", dur: null, reason: "run result lost: pid 4242 ended after a host restart and its exit status could not be collected" }),
              mk({ status: "skipped", name: "sign:artifacts", dur: null, skip: "branch_not_selected" }),
              mk({ kind: "agent", phase: "awaiting_input", provider: "claude", since: "2m 4s", name: "agent:review" })].map((n) => (
              <div key={n.name} style={{ display: "flex", flexDirection: "column", gap: 8, width: 300 }}>
                <NodeBox node={n} dimmed={n.status === "skipped"} caption={n.status === "running" ? "awaiting_input" : n.status} />
                <pre style={{ margin: 0, padding: "6px 8px", whiteSpace: "pre-wrap", fontFamily: "var(--tasty-font-ui)", fontSize: "var(--tasty-font-size-caption)",
                  color: "var(--tasty-text-primary)", background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
                  borderRadius: "var(--tasty-radius-sm)" }}>{nodeTitle(n)}</pre>
              </div>
            ))}
          </Stage>
          <Stage style={{ padding: 0, display: "block" }}>
            <div style={{ display: "flex", height: 360, background: "var(--tasty-bg-panel)" }}>
              <DagDetail dag={{ nodes: [mk({ id: "a", kind: "agent", phase: "awaiting_input", provider: "claude", since: "2m 4s", name: "agent:review" })] }} id="a" onClose={() => {}} onSelect={() => {}} />
              <DagDetail dag={{ nodes: [mk({ id: "u", status: "unknown", name: "deploy:site", dur: null, reason: "run result lost: pid 4242 ended after a host restart and its exit status could not be collected" })] }} id="u" onClose={() => {}} onSelect={() => {}} />
            </div>
          </Stage>
          <Note><b>Labels (i18n).</b> Why: · 이유: · 理由: — Why unknown · 알 수 없는 이유 · 不明の理由 — “Retry or cancel it to let the graph continue.” · “다시 실행하거나 취소해야 그래프가 이어집니다.” · “再実行またはキャンセルするとグラフが続行します。” — “Waiting for a person in the {"{provider}"} session” · “{"{provider}"} 세션이 사람의 입력을 기다리는 중” · “{"{provider}"} セッションが入力を待っています”. The reason text is the host's free text, shown as is.</Note>
        </Spec>
      </Section>

      <Section id="edges" title="Edges · 5 relations">
        <Spec id="dependency-edges" title="Dependency edges"
          when={<>Relation is carried by <b>dash pattern and colour together</b>: <code>depends_on</code> solid neutral, <code>fallback</code> long-dash attention, <code>reduce</code> fine-dash info, <code>binding</code> dash-dot data (teal, 2026-10-07), <code>transition</code> long-dash route (lavender, 2026-10-07). When a pair has both <code>depends_on</code> and <code>binding</code>, <b>only the binding is drawn</b> — a binding already implies the order. A <code>one_of</code> binding draws one binding edge per source. Every edge ends in an arrowhead at the <b>dependent</b> task, so the direction reads as "waits for".</>}>
          <Stage variant="grid" style={{ display: "flex", gap: 32, padding: 20, alignItems: "flex-start" }}>
            {Object.keys(DAG_REL).map((r) => <EdgeSpecimen key={r} rel={r} />)}
          </Stage>
          <Meta
            specs={[["routing", "orthogonal, 4px elbow"], ["width", "1px"], ["fallback dash", "6 3"], ["reduce dash", "2 3"], ["binding dash", "8 2 2 2"], ["transition dash", "10 4"], ["same pair", "binding replaces depends_on"], ["detail label", "binding → “binds input” · transition → “transition”"], ["arrow", "8px triangle at target"], ["selected node", "its edges take the accent"]]}
            tokens={[{ tok: "--tasty-dag-edge-depends", use: "depends_on", color: "var(--tasty-dag-edge-depends)" }, { tok: "--tasty-dag-edge-fallback", use: "fallback", color: "var(--tasty-dag-edge-fallback)" }, { tok: "--tasty-dag-edge-reduce", use: "reduce", color: "var(--tasty-dag-edge-reduce)" }, { tok: "--tasty-dag-edge-binding", use: "binding → accent-data", color: "var(--tasty-dag-edge-binding)" }, { tok: "--tasty-dag-edge-transition", use: "transition → accent-route", color: "var(--tasty-dag-edge-transition)" }, { tok: "--tasty-dag-edge-dim-opacity", use: "dead path" }]} />
          <Do>Recede the <b>dead path</b>: an edge leaving a failed or cancelled task, or entering a skipped one, loses its hue and reads <span className="tok">dag-edge-dim</span>, solid neutral ink at 3:1 or better. Its dash stays, so the relation still reads. The failure stays loud; everything it stranded recedes.</Do>
        </Spec>
      </Section>

      <Section id="routes" title="Transitions · not selected">
        <Spec id="transition-selection-four-states-on-one-edge-style" title="Transition selection — four states on one edge style"
          when={<>A transition edge always keeps its lavender long dash; its <b>selection</b> changes weight and ink only. <b>pending</b> (producer not finished) = 1px, full opacity. <b>selected</b> = <b>2px</b> (<span className="tok">--tasty-dag-edge-selected-width</span>). <b>not_selected</b> = 1px in <span className="tok">--tasty-dag-edge-dim</span>, a solid neutral with a 3:1 floor. <b>unavailable</b> (producer failed) = the same dead-path ink, like any edge leaving a failed task. The hue drops and the long dash stays, so the relation still reads. Nothing is hidden: the unchosen route stays readable, on a running run and on a finished one.</>}>
          <Stage variant="grid" style={{ display: "flex", gap: 32, padding: 20, alignItems: "flex-start", flexWrap: "wrap" }}>
            {[["pending", 1, 1], ["selected", 2, 1], ["not_selected", 1, 0.4], ["unavailable", 1, 0.4]].map(([s, w, o]) => (
              <div key={s} style={{ display: "flex", flexDirection: "column", gap: 6, alignItems: "center" }}>
                <svg width="120" height="56" viewBox="0 0 120 56" aria-hidden="true">
                  <path d={elbow({ x: 12, y: 8 }, { x: 108, y: 48 }, true, 4)} fill="none" stroke={o < 1 ? "var(--tasty-dag-edge-dim)" : "var(--tasty-dag-edge-transition)"} strokeWidth={w} strokeDasharray="10 4" />
                  <polygon points="104,41 112,41 108,48" fill={o < 1 ? "var(--tasty-dag-edge-dim)" : "var(--tasty-dag-edge-transition)"} />
                </svg>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-dag-edge-transition)" }}>{s}</span>
              </div>
            ))}
          </Stage>
          <Stage variant="grid" style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 20, alignItems: "flex-start" }}>
            <NodeBox node={mk({ status: "succeeded", name: "route:pick", kind: "custom", dur: "1s" })} caption="producer · succeeded" />
            <NodeBox node={mk({ status: "running", name: "deploy:canary" })} caption="branch selected" />
            <NodeBox node={mk({ status: "skipped", name: "deploy:full", dur: null })} dimmed caption="skipped — branch not selected" />
            <NodeBox node={mk({ status: "skipped", name: "notify:ops", kind: "custom", dur: null })} dimmed caption="skipped — upstream unavailable" />
          </Stage>
          <Meta
            specs={[["pending", "1px · full opacity"], ["selected", "2px · dag-edge-selected-width"], ["not_selected", "1px · dag-edge-dim · long dash"], ["unavailable", "1px · dag-edge-dim, as from a failed task"], ["not-selected node", "skipped look (dimmed card, ⊘ glyph — canonical for both skip reasons; − is cancelled) · label NOT SELECTED · tooltip = kit nodeTitle(): “{name} — not selected” + “Why: Not selected by the upstream result” (2026-10-09)"], ["upstream-unavailable node", "skipped look · label SKIPPED · tooltip = nodeTitle(): “{name} — skipped” + “Why: An upstream task did not succeed”"], ["rollup", "succeeded + not selected only → succeeded"], ["skip count", "DAG list row + DAG tab header, after done/total: “{done} / {total} · {n} skipped ({k} not selected)” — shown only when n > 0, parenthesis only when k > 0 · same mono caption, dag-row-count-fg · not clickable"], ["filter", "no new list filter"], ["legend", "none — the relation reads from the dependency rows in Node detail; the canvas carries no legend"]]}
            tokens={[{ tok: "--tasty-dag-edge-transition", use: "route", color: "var(--tasty-dag-edge-transition)" }, { tok: "--tasty-dag-edge-selected-width", use: "→ focus-ring-width 2" }, { tok: "--tasty-dag-edge-dim", use: "not selected / unavailable", color: "var(--tasty-dag-edge-dim)" }, { tok: "--tasty-dag-status-skipped", use: "both skip reasons", color: "var(--tasty-dag-status-skipped)" }]} />
          <Note>A branch that was not taken is a normal outcome, so it gets <b>no new colour</b>: it shares the skipped card and differs only in its spelled label and tooltip. i18n: <code>dag.status.not_selected</code> “not selected” · <code>dag.why.branch_not_selected</code> · <code>dag.why.upstream_unavailable</code> (2026-10-09 — replaces the retired <code>dag.skip.*</code> keys) · <code>dag.rel.binding</code> “binds input” · <code>dag.rel.transition</code> “transition”.</Note>
        </Spec>
      </Section>

      <Section id="chrome" title="Canvas chrome">
        <Spec id="zoom-cluster-minimap" title="Zoom cluster + minimap"
          when={<>Bottom-right overlay, 8px off the canvas edge: minimap above, zoom cluster below (− · % · + · fit · direction). Below <b>560px</b> of surface width the minimap is dropped; below <b>400px</b> the percentage readout goes too, leaving five 28px targets.</>}>
          <Stage variant="solo center" style={{ gap: 24, padding: 24, alignItems: "flex-end" }}>
            <Minimap g={dagLayout(DAG_BUILD, "td")} dag={DAG_BUILD} z={0.8} ox={40} oy={20} vw={520} vh={360} dir="td" />
            <div style={{ display: "flex", flexDirection: "column", gap: 12, alignItems: "flex-start" }}>
              <ZoomCluster z={0.8} onZoom={() => {}} onFit={() => {}} dir="td" onDir={() => {}} />
              <ZoomCluster z={0.8} compact onZoom={() => {}} onFit={() => {}} dir="td" onDir={() => {}} />
            </div>
          </Stage>
          <Meta
            specs={[["cluster", "28px row · 1px border"], ["inset", "8px from canvas edge"], ["minimap", "160 × 112"], ["viewport", "1px accent rect"], ["minimap cutoff", "surface < 560px"], ["compact cutoff", "surface < 400px"]]}
            tokens={[{ tok: "--tasty-dag-chrome-bg", use: "cluster fill", color: "var(--tasty-dag-chrome-bg)" }, { tok: "--tasty-dag-minimap-viewport", use: "viewport rect", color: "var(--tasty-dag-minimap-viewport)" }, { tok: "--tasty-dag-minimap-min-surface", use: "560 cutoff" }]} />
          <Note>The minimap paints each node in its <b>status colour</b>, so it doubles as a health strip: a red block anywhere in the graph is visible without zooming out.</Note>
        </Spec>
      </Section>

      <Section id="runner" title="Runner badge">
        <Spec id="host-runner-state" title="Host runner state"
          when={<>Four values in one pill: on/off, crashed, <code>ready_count</code>, <code>running_count</code>. The case that must be <b>noticed</b> is <b>runner stopped with work ready</b> — nothing is advancing the graph — so it takes the warning tone and carries the resume hint beside it.</>}>
          <Stage variant="solo" style={{ flexDirection: "column", alignItems: "flex-start", gap: 12, padding: 20 }}>
            <RunnerBadge runner={{ running: true, crashed: false, ready: 3, active: 2 }} />
            <RunnerBadge runner={{ running: true, crashed: false, ready: 0, active: 0 }} />
            <RunnerBadge runner={{ running: false, crashed: false, ready: 4, active: 0 }} />
            <RunnerBadge runner={{ running: false, crashed: false, ready: 0, active: 0 }} />
            <RunnerBadge runner={{ running: false, crashed: true, ready: 2, active: 0 }} />
          </Stage>
          <Meta
            specs={[["pill", "22px · radius 2 · 1px border"], ["dot", "StatusDot, pulses only while work runs"], ["counts", "mono 11"], ["hint", "hidden on a narrow header"]]}
            tokens={[{ tok: "--tasty-dag-runner-stalled-fg", use: "stopped + ready", color: "var(--tasty-dag-runner-stalled-fg)" }, { tok: "--tasty-dag-runner-crashed-fg", use: "crashed", color: "var(--tasty-dag-runner-crashed-fg)" }, { tok: "--tasty-dag-runner-idle-fg", use: "stopped, no work", color: "var(--tasty-dag-runner-idle-fg)" }]} />
          <Note>"Stopped with no work" is <b>not</b> a warning — it is the resting state of a finished graph, so it stays muted. Only stopped-<i>with</i>-ready earns the yellow.</Note>
        </Spec>
      </Section>

      <Section id="rows" title="DAG list row">
        <Spec id="one-row-per-dag-across-all-workspaces" title="One row per DAG — across all workspaces"
          when={<>A <code>ListCtrl</code> row (36px, icon + label + description) plus the one new part: a trailing cluster of <b>origin tag</b>, <b>rollup state</b> (same 8-state vocabulary as a task) and a mono <b>done/total</b>. Progress is text, not a bar — at this density <code>7/12</code> is exact where a 60px bar is only a suggestion.</>}>
          <Stage style={{ padding: 0, display: "block" }}>
            <div style={{ background: "var(--tasty-bg-panel)" }}>
              <ListCtrl items={dagRowItems(DAG_LIST)} onSelect={() => {}} />
            </div>
          </Stage>
          <Meta
            specs={[["row", "36px min · ListCtrl density"], ["label", "13 — DAG name"], ["description", "11 — workspace · updated"], ["trailing", "origin tag · rollup · n/total · skip count when > 0"], ["scope", "all workspaces, filterable"]]}
            tokens={[{ tok: "--tasty-dag-row-height", use: "36 row", }, { tok: "--tasty-dag-row-count-fg", use: "n/total", color: "var(--tasty-dag-row-count-fg)" }, { tok: "--tasty-listctrl-row-bg-hover", use: "hover" }]} />
          <Note><b>Rollup (2026-10-07).</b> Seven values, in filter order: {DAG_ROLLUP_ORDER.join(" · ")}. <b>partially_failed</b> (◒, peach <code>--tasty-dag-status-partially-failed</code>) = no more progress, an unrecovered failure, and at least one branch succeeded to its end. Precedence is progress first: running → ready → waiting-that-can-run → partially_failed | failed → succeeded | skipped. <code>dagRollup(DAG_PARTIAL.nodes)</code> = <code>{dagRollup(DAG_PARTIAL.nodes)}</code>. Its own filter option, between succeeded and failed. done/total counts its failed tasks as done, same as failed. Label: Partially failed · 일부 실패 · 一部失敗.</Note>
          <Note>The workspace name is part of the description line, not a separate column — the popup lists every workspace, and a column would waste the width the DAG name needs.</Note>
        </Spec>
      </Section>

      <Section id="detail" title="Node detail">
        <Spec id="selected-task-side-panel-bottom-sheet" title="Selected task — side panel · bottom sheet"
          when={<>Selecting a node opens a <b>288px right panel</b> on a wide surface and a <b>220px bottom sheet</b> when the surface is narrow (&lt; 640px) or inside the popup. Failure output can run to dozens of lines, so the error tail is its own bounded, scrollable, copyable block — never an expanding wall that pushes the graph off screen.</>}>
          <Stage style={{ padding: 0, display: "block" }}>
            <div style={{ display: "flex", height: 460, background: "var(--tasty-bg-panel)" }}>
              <DagDetail dag={DAG_BUILD} id="build_linux" onClose={() => {}} onSelect={() => {}} />
              <DagDetail dag={DAG_BUILD} id="unit" onClose={() => {}} onSelect={() => {}} />
            </div>
          </Stage>
          <Meta
            specs={[["panel", "288 wide · 12 padding"], ["sheet", "220 tall (narrow)"], ["error tail", "max 160, scrolls, copy button"], ["output tail", "max 112, scrolls"], ["deps", "22px rows, click to jump"]]}
            tokens={[{ tok: "--tasty-dag-detail-bg", use: "panel fill", color: "var(--tasty-dag-detail-bg)" }, { tok: "--tasty-dag-detail-log-bg", use: "log bed", color: "var(--tasty-dag-detail-log-bg)" }, { tok: "--tasty-dag-detail-log-max-height", use: "160 error tail" }]} />
          <Note>Dependency rows are the graph's keyboard-free navigation: each row shows the upstream task's glyph + relation and jumps the selection there, so a failure can be walked back to its cause without hunting on the canvas.</Note>
        </Spec>
      </Section>

      <Section id="states" title="Empty · cycle">
        <Spec id="empty-states-and-the-cycle-warning" title="Empty states and the cycle warning"
          when={<>Three non-happy states: <b>no DAGs in this workspace</b> (surface), <b>no filter match</b> (popup list), and <b>cycle detected</b> — a warning strip pinned to the canvas top. The cyclic graph <b>keeps rendering</b> behind it: this surface observes, it never hides state.</>}>
          <Stage variant="grid" style={{ flexDirection: "column", gap: 16, padding: 20, alignItems: "stretch" }}>
            <div style={{ display: "flex", minHeight: 160, border: "var(--tasty-border-width) solid var(--tasty-border-default)",
              borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-panel)" }}>
              <DagEmpty variant="surface" />
            </div>
            <div style={{ display: "flex", minHeight: 160, border: "var(--tasty-border-width) solid var(--tasty-border-default)",
              borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-panel)" }}>
              <DagEmpty variant="search" query="deploy" />
            </div>
            <CycleBanner ids={DAG_CYCLE.cycle} />
          </Stage>
          <Stage style={{ padding: 0, height: 320, display: "block" }}>
            <div style={{ height: 320, display: "flex" }}><DagCanvas dag={DAG_CYCLE} dir="td" /></div>
          </Stage>
          <Meta
            specs={[["banner", "28px · pinned to canvas top"], ["banner copy", "names the cycle path"], ["empty A", "surface — how a DAG appears"], ["empty B", "search — echoes the query"]]}
            tokens={[{ tok: "--tasty-dag-cycle-bg", use: "banner wash", color: "var(--tasty-dag-cycle-bg)" }, { tok: "--tasty-dag-cycle-fg", use: "banner text", color: "var(--tasty-dag-cycle-fg)" }, { tok: "--tasty-text-disabled", use: "empty glyph" }]} />
        </Spec>
        <Spec id="dag-list-popup-empty-and-no-match" title="DAG list popup — empty and no match"
          when={<>The popup list uses the <b>same DagEmpty</b> as the surface (one shared function): glyph, title, hint, stacked in the <b>vertical centre</b> of the list area (not the 1/3 point). <b>No match</b> is the <code>search</code> variant and echoes the query. <b>No DAG at all</b> is its own <code>popup</code> variant: the popup spans every workspace, so the surface hint with a workspace-scoped command does not fit; it keeps the app's two strings.</>}>
          <Stage variant="solo" style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 16, padding: 20 }}>
            {[["popup — no DAGs", "popup", ""], ["popup — filter “deploy”", "search", "deploy"]].map(([cap, v, q]) => (
              <div key={cap} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <div style={{ display: "flex", height: 220, border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-panel)" }}><DagEmpty variant={v} query={q} /></div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["shared", "DagEmpty — surface · search · popup"], ["position", "centred in the list area, both axes"], ["no match", "search glyph · “No DAGs match “{q}”” · “Clear the filter or widen the scope to all workspaces.”"], ["no DAGs", "gitTree glyph · “No DAGs yet” · “Tasks grouped by dependency appear here.” (existing keys)"], ["glyph", "24 · text-disabled"]]}
            tokens={[{ tok: "--tasty-text-disabled", use: "glyph", color: "var(--tasty-text-disabled)" }, { tok: "--tasty-text-secondary", use: "title", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-muted", use: "hint", color: "var(--tasty-text-muted)" }]} />
          <Note>i18n: the search title is new — <code>dag.popup.no_match</code> “No DAGs match “{"{q}"}”” / ““{"{q}"}”와 일치하는 DAG 없음” / ““{"{q}"}” に一致する DAG はありません” · hint <code>dag.popup.no_match_hint</code> “Clear the filter or widen the scope to all workspaces.” / “필터를 지우거나 범위를 모든 워크스페이스로 넓히세요.” / “フィルターを消すか、範囲をすべてのワークスペースに広げてください。”. The old “No matching DAGs” / “Try a different filter.” go.</Note>
        </Spec>
        <Spec id="dag-list-popup-no-match-without-a-query" title="DAG list popup — no match without a query"
          when={<>The list can empty with <b>no search text</b>: the status filter or <b>This workspace only</b> alone filters everything out. There is no query to echo, so this is its own <code>filtered</code> variant: <b>filter</b> glyph (the cause is a filter, not the search field), a title without quotes, and the <b>same hint</b> as the search variant. Variant choice: trimmed query non-empty → <code>search</code> (echo the query only, even when status/scope filters are also on — the hint already says “clear the filter”); trimmed query empty and filters active → <code>filtered</code>; no DAG at all → <code>popup</code>.</>}>
          <Stage variant="solo" style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 16, padding: 20 }}>
            {[["Mocha — status = Failed, no query", null, "filtered", ""], ["Latte — workspace only, no query", "latte", "filtered", ""], ["Mocha — query “deploy” + status filter", null, "search", "deploy"]].map(([cap, th, v, q]) => (
              <div key={cap} {...(th ? { "data-theme": th } : {})} style={{ display: "flex", flexDirection: "column", gap: 6, background: "var(--tasty-bg-panel)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <div style={{ display: "flex", height: 220, border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-panel)" }}><DagEmpty variant={v} query={q} /></div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["variant", "DagEmpty filtered"], ["when", "query.trim() empty · status filter ≠ all OR workspace-only on · 0 rows"], ["glyph", "filter · 24 · text-disabled"], ["title", "“No DAGs match the current filters”"], ["hint", "same key as search: dag.popup.no_match_hint"], ["query + filters", "search variant, echoes the query only"]]}
            tokens={[{ tok: "--tasty-text-disabled", use: "glyph", color: "var(--tasty-text-disabled)" }, { tok: "--tasty-text-secondary", use: "title", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-muted", use: "hint", color: "var(--tasty-text-muted)" }]} />
          <Note>i18n (new): <code>dag.popup.no_match_filtered</code> “No DAGs match the current filters” / “현재 필터와 일치하는 DAG 없음” / “現在のフィルターに一致する DAG はありません”. Hint reuses <code>dag.popup.no_match_hint</code>. The temporary <code>dag_list.empty_filtered*</code> (“No matching DAGs” / “Try a different filter.”) can now be removed.</Note>
        </Spec>
      </Section>

      <Section id="surfaces" title="Surface · popup">
        <Spec id="full-tab-surface-wide-and-narrow" title="Full-tab surface — wide and narrow"
          when={<>A whole terminal tab. Header: DAG name + <code>{"{done} / {total} done"}</code> (spaces around the slash in every language, 2026-10-09) (mono caption, <code>--tasty-dag-header-count-fg</code> = the list row counter, skip suffix after it, 2026-10-07), the DAG <code>Select</code>, the runner badge, refresh. Under <b>640px</b> the header wraps to two rows (identity above, controls below), the runner hint drops, the minimap goes, and the detail panel becomes a bottom sheet.</>}>
          <Stage style={{ padding: 16, display: "flex", gap: 16, alignItems: "stretch" }}>
            <div style={{ flex: 1, minWidth: 0, height: 520, border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
              borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <DagSurface dags={[DAG_BUILD, DAG_INDEX, DAG_DENSE, DAG_CYCLE]} />
            </div>
            <div style={{ flex: "none", width: 320, height: 520, border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
              borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <DagSurface dags={[DAG_BUILD, DAG_INDEX]} narrow />
            </div>
          </Stage>
          <Meta
            specs={[["header", "8/12 padding · wraps < 640"], ["canvas", "all remaining space"], ["detail", "288 side → 220 sheet"], ["narrow floor", "320px, nothing clipped"], ["DAG switch", "instant swap + auto-fit"]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "header band", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-separator", use: "header hairline" }, { tok: "--tasty-dag-detail-sheet-height", use: "220 sheet" }]} />
          <Note>Switching DAG in the header <code>Select</code> replaces the canvas instantly (0ms) and re-fits — a new graph has no position to preserve, so fit is the honest default. Within one DAG the view never re-fits on its own.</Note>
        </Spec>

        <Spec id="workspace-popup-list-single-dag" title="Workspace popup — list ⇄ single DAG"
          when={<>A movable 560 × 460 popup. The list carries search, a <b>this workspace only</b> toggle and a status filter; picking a row swaps the whole area for that DAG's canvas via <code>DrillDown</code> — instant, with a back bar that keeps the runner badge in view. The bottom sheet holds node detail here, because 560px has no room for a side panel.</>}>
          <Stage style={{ padding: 0, display: "block" }}><PopupStage /></Stage>
          <Meta
            specs={[["frame", "560 × 460 · resizable"], ["titlebar", "28px drag strip"], ["list rows", "36px · ~8 visible"], ["swap", "DrillDown, 0ms"], ["dismiss", <>outside click · <span className="ic">Esc</span> · ×</>]]}
            tokens={[{ tok: "--tasty-dag-popup-width", use: "560 frame" }, { tok: "--tasty-dag-popup-height", use: "460 frame" }, { tok: "--tasty-drilldown-backbar-height", use: "36 back bar" }, { tok: "--tasty-shadow-modal", use: "lift" }]} />
        </Spec>

        <Spec id="detail-view-the-header-keeps-the-runner-badge-only" title="Detail view — the header keeps the runner badge only"
          when={<>The popup's list → detail flow reuses the graph screen, which drags the graph header along: the <b>runner badge</b>, the <b>DAG selector</b> and the <b>zoom cluster</b> all arrive in a view that shows one node — and the back bar's actions slot sits empty beneath them. Settled: <b>no second header</b>. The <b>back bar is the detail's chrome</b>, and its <b>actions slot</b> carries the <b>compact zoom cluster</b> (beside the graph it scales) and the <b>runner badge</b> (it describes the run the node belongs to). The <b>DAG selector is hidden</b>: the back-bar title already names the DAG, and switching DAGs under an open node detail has no meaning. Back restores the list, its header, and the selection.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>list — full header</div>
              <DagWindow />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>detail — open a row above: the back bar carries zoom + runner badge</div>
              <DagWindow />
            </div>
          </Stage>
          <Meta
            specs={[["frame", "560×460 — unchanged"], ["detail header", "none — the back bar is the chrome"], ["back bar actions", "compact zoom cluster + runner badge"], ["DAG selector", "hidden in detail"], ["back bar", <>36 — <span className="tok">--tasty-drilldown-backbar-height</span></>], ["back", "restores the list, keeps the selection"], ["close / reset", "unchanged host contract"]]}
            tokens={[{ tok: "--tasty-drilldown-backbar-height", use: "back bar" }, { tok: "--tasty-dag-popup-width", use: "560 frame" }, { tok: "--tasty-dag-popup-height", use: "460 frame" }]} />
          <Note>The actions slot was empty in the current build; this fills it rather than inventing a second bar. If a future detail view needs no zoom, the slot simply goes back to empty.</Note>
        </Spec>
      </Section>
    </>
  );
}

export const page = {
  active: "dag",
  nav: NAV,
  head: {
  title: "Task DAG",
  intro: "Inspect a task graph in a tab or workspace popup. Compare execution states, edges, lists, and detail views. Create and edit tasks through the CLI.",
  
},
  content: <Page />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}
