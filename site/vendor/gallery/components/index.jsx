import React from "react";
import { PageStates } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Icon as CIcon, Button, IconButton, Badge, BadgeGroup, Tag, Kbd, Input, Select, MultiSelect, AutoComplete, Checkbox, Switch, Tab, TreeRow, MenuItem, StatusDot, Toast, Table, Spinner, HelpHint, Tooltip, ListCtrl, CenterState } from "../../components/index.js";
import { GalleryScrollbar } from "./scrollbar.jsx";
import { GallerySearchableSelect } from "./searchable-select.jsx";
import { Section, Spec, Stage, Cluster, Meta, Note, Do, Dont, GIcon } from "../parts/specimen.jsx";

// Tasty Gallery — Components. Live, interactive specimens of the DS
// primitives. Hover / focus work for real on the page; persistent
// states (active, disabled) are shown explicitly. Each carries a
// "when to use" note, the layout spec, and the tokens it consumes.





// Selected row (2026-11-05). The row re-points tier-3 dot tokens, as TOKENS.md describes, so the dot keeps one source.
const SEL_ROW_VARS = {
  "--tasty-status-dot-idle": "var(--tasty-status-dot-idle-active)",
  "--tasty-status-dot-success": "var(--tasty-status-dot-success-active)",
  "--tasty-status-dot-warning": "var(--tasty-status-dot-warning-active)",
  "--tasty-status-dot-danger": "var(--tasty-status-dot-danger-active)",
  "--tasty-status-dot-agent": "var(--tasty-status-dot-agent-active)",
  "--tasty-status-dot-needs-input": "var(--tasty-status-dot-needs-input-active)",
  "--tasty-status-dot-completion": "var(--tasty-status-dot-completion-active)",
  "--tasty-status-dot-attached-ring": "var(--tasty-status-dot-attached-ring-active)",
};
function SelRow({ active, children }) {
  return (
    <div data-sel-row={active ? "active" : "rest"} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0, height: "var(--tasty-control-height)", padding: "0 var(--tasty-space-sm)", borderRadius: "var(--tasty-radius-sm)", whiteSpace: "nowrap", overflow: "hidden", fontSize: "var(--tasty-font-size-body)",
      background: active ? "var(--tasty-surface-active)" : "transparent", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", ...(active ? SEL_ROW_VARS : null) }}>{children}</div>
  );
}
const SelGlyph = ({ name, color }) => <span style={{ display: "inline-flex", flex: "none", color }}><CIcon name={name} size="var(--tasty-icon-size-sm)" /></span>;
const SelTag = ({ variant, children }) => <Tag variant={variant} style={{ flex: "none", color: "var(--tasty-tag-" + variant + "-fg)", ...(variant === "attached" ? { borderColor: "color-mix(in srgb, var(--tasty-accent-attached) 40%, transparent)" } : null) }}>{children}</Tag>;
const SEL_CASES = [
  ["Port popup · favorite star", (a) => <><SelGlyph name="starFill" color={a ? "var(--tasty-port-star-on-active)" : "var(--tasty-port-star-on)"} /><SelGlyph name="star" color={a ? "var(--tasty-port-star-off-active)" : "var(--tasty-port-star-off)"} /><span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)" }}>127.0.0.1:5173</span></>],
  ["Settings list · plugin dot", (a) => <><span style={{ flex: "none", width: "var(--tasty-size-6)", height: "var(--tasty-size-6)", borderRadius: "var(--tasty-radius-pill)", background: a ? "var(--tasty-accent-agent-on-active)" : "var(--tasty-accent-agent)" }} /><span>git-helper</span></>],
  ["File handlers · plugin glyph", (a) => <><SelGlyph name="plug" color={a ? "var(--tasty-accent-agent-on-active)" : "var(--tasty-accent-agent)"} /><span>Open with image viewer</span></>],
  ["Sidebar · dot, mirror glyph, Remote Tag", (a) => <><StatusDot status="running" /><SelGlyph name="remote" color={a ? "var(--tasty-workspace-mirror-fg-active)" : "var(--tasty-workspace-mirror-fg)"} /><span>Project A</span><SelTag variant="info">remote</SelTag></>],
  ["Remote attach · chips", () => <><StatusDot status="idle" attached /><span>build</span><SelTag variant="attached">in use</SelTag><SelTag variant="warning">inactive</SelTag></>],
  ["Explorer, file picker · kind glyph, match run", (a) => <><SelGlyph name="folder" color={a ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)"} /><SelGlyph name="image" color={a ? "var(--tasty-text-on-active)" : "var(--tasty-accent-info-raised)"} /><span>dia<span data-match-run="" style={a ? { color: "var(--tasty-explorer-match-fg-active)", textDecoration: "underline", textDecorationThickness: "var(--tasty-autocomplete-match-underline-width)", textUnderlineOffset: "var(--tasty-autocomplete-match-underline-offset)" } : { color: "var(--tasty-explorer-match-fg)" }}>gram</span>.png</span></>],
  ["Shortcut · Kbd +", (a) => <><span style={{ flex: 1 }}>Split pane right</span><Kbd keys="Ctrl" /><span style={{ color: a ? "var(--tasty-kbd-plus-fg-active)" : "var(--tasty-kbd-plus-fg)", fontSize: "var(--tasty-font-size-micro)", fontFamily: "var(--tasty-font-mono)" }}>+</span><Kbd keys="D" /></>],
  ["Status dots", () => <><StatusDot status="running" /><StatusDot status="waiting" /><StatusDot status="error" /><StatusDot status="agent" /><StatusDot status="needs-input" /><StatusDot status="completion" /><StatusDot status="idle" /></>],
];
function SelRowGrid({ label, theme }) {
  return (
    <div data-theme={theme} style={{ flex: "1 1 var(--tasty-size-320)", minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{label}</span>
      {SEL_CASES.map(([cap, draw]) => (
        <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
          <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
          <SelRow active={false}>{draw(false)}</SelRow>
          <SelRow active>{draw(true)}</SelRow>
        </div>
      ))}
    </div>
  );
}
// Ink over its composited ground: label, ground, overlay, ink, sample, Mocha, Latte.
const SEL_INK = [
  ["Favorite star", "surface-active", null, "port-star-on-active", "★", "5.25", "3.75"],
  ["Plugin dot, glyph", "surface-active", null, "accent-agent-on-active", "●", "3.29", "3.48"],
  ["Mirror glyph", "surface-active", null, "workspace-mirror-fg-active", "●", "4.29", "3.48"],
  ["Dot, running", "surface-active", null, "status-dot-success-active", "●", "4.49", "3.50"],
  ["Dot, waiting, needs input", "surface-active", null, "status-dot-warning-active", "●", "5.25", "3.75"],
  ["Dot, error", "surface-active", null, "status-dot-danger-active", "●", "3.28", "3.42"],
  ["Dot, agent", "surface-active", null, "status-dot-agent-active", "●", "3.29", "3.48"],
  ["Dot, completion", "surface-active", null, "status-dot-completion-active", "●", "3.17", "3.83"],
  ["Dot, idle", "surface-active", null, "status-dot-idle-active", "●", "4.62", "5.39"],
  ["Attached ring", "surface-active", null, "status-dot-attached-ring-active", "○", "3.73", "3.78"],
  ["Match run on selected row", "surface-active", null, "explorer-match-fg-active", "gram", "4.62", "5.39"],
  ["Kbd + on selected row", "surface-active", null, "kbd-plus-fg-active", "+", "4.62", "5.39"],
  ["Match run at rest, menu", "surface-raised", null, "autocomplete-match-fg", "gram", "5.97", "5.37"],
  ["Match run, pointer hover", "surface-raised", "overlay-hover", "autocomplete-match-fg", "gram", "4.66", "4.52"],
  ["Match run at rest, panel", "bg-panel", null, "explorer-match-fg", "gram", "7.79", "7.32"],
  ["Tag attached", "tag-bg", null, "tag-attached-fg", "in use", "7.03", "5.29"],
  ["Muted on bg-app", "bg-app", null, "text-muted", "muted", "8.42", "4.73"],
  ["Muted on bg-sidebar", "bg-sidebar", null, "text-muted", "muted", "7.89", "5.14"],
  ["Muted on bg-panel", "bg-panel", null, "text-muted", "muted", "7.37", "5.53"],
  ["Kbd + on bg-app", "bg-app", null, "kbd-plus-fg", "+", "8.42", "6.04"],
  ["Kbd + on bg-sidebar", "bg-sidebar", null, "kbd-plus-fg", "+", "7.89", "6.57"],
  ["Kbd + on bg-panel", "bg-panel", null, "kbd-plus-fg", "+", "7.37", "7.06"],
  ["Kbd + on surface-raised", "surface-raised", null, "kbd-plus-fg", "+", "5.65", "5.17"],
];
function SelInkTable({ label, theme }) {
  const col = theme === "latte" ? 6 : 5;
  return (
    <div data-theme={theme} style={{ flex: "1 1 var(--tasty-size-320)", minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{label}</span>
      {SEL_INK.map((r) => (
        <div key={r[0]} data-ink-row={r[0]} data-ink-ground={r[1]} data-ink-overlay={r[2] || ""} data-ink-token={r[3]} data-ink-value={r[col]} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)" }}>{r[0]}</span>
          <span style={{ flex: "none", padding: "var(--tasty-space-xs) var(--tasty-space-sm)", borderRadius: "var(--tasty-radius-sm)", fontSize: "var(--tasty-font-size-caption)", whiteSpace: "nowrap",
            color: "var(--tasty-" + r[3] + ")", backgroundColor: "var(--tasty-" + r[1] + ")",
            backgroundImage: r[2] ? "linear-gradient(var(--tasty-" + r[2] + "), var(--tasty-" + r[2] + "))" : "none" }}>{inkSample(r[4])}</span>
          <span style={{ flex: "none", width: "var(--tasty-size-32)", textAlign: "right", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)" }}>{r[col]}</span>
        </div>
      ))}
    </div>
  );
}


// Workspace row (expanded sidebar) — attached-ring slot specimen. geom="product"
// reproduces the rejected 4 / 8 / 4 geometry for comparison only.
function WsColG({ children }) {
  return <div style={{ width: "var(--tasty-size-200)", display: "flex", flexDirection: "column", background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>{children}</div>;
}
function WsRowG({ name, active, hover, attached, pill, sub, geom = "settled" }) {
  const prod = geom === "product";
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: prod ? "var(--tasty-space-xs)" : "var(--tasty-workspace-dot-gap)",
      padding: prod ? "var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-space-xs)" : "var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-workspace-row-padding-x)",
      background: active ? "var(--tasty-surface-active)" : hover ? "var(--tasty-overlay-hover)" : "transparent",
      boxShadow: active ? "inset var(--tasty-workspace-row-active-bar-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      {/* slot: width = workspace-dot-slot; height = the title label's own line box (never taller → never grows the row) */}
      <span style={{ flex: "none", width: prod ? "var(--tasty-status-dot-size)" : "var(--tasty-workspace-dot-slot)", height: "calc(var(--tasty-font-size-body) * var(--tasty-line-height-ui))", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
        <StatusDot status="running" attached={attached} />
      </span>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-label-detail-gap)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-medium)", lineHeight: "var(--tasty-line-height-ui)", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{name}</span>
        {pill && <span style={{ alignSelf: "flex-start" }}><Tag variant="info">remote</Tag></span>}
        {sub && <span style={{ fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{sub}</span>}
      </div>
    </div>
  );
}

// CenterState — the one centred empty / loading / error block.
const CS_COPY = {
  fp: { loading: ["Loading folder", ""], empty: ["This folder is empty", "Files you add here appear in this list."], error: ["Could not read this folder", "Permission denied (os error 13)"] },
  scripts: { loading: ["Loading scripts", ""], empty: ["No scripts", "Add a Lua script to run it on a lifecycle event."], error: ["Could not load scripts", "~/.config/tasty/scripts is not readable"] },
};
function CenterStateG({ variant = "empty", host = "fp", action = false }) {
  const [title, sub] = CS_COPY[host][variant];
  return <CenterState variant={variant} glyph={host === "scripts" ? "scriptFile" : "folderOpen"} title={title} sub={sub}
    action={action ? <Button variant="secondary" size="sm" leadingIcon={<CIcon name="refresh" size="var(--tasty-icon-size-sm)" />}>Retry</Button> : null} />;
}

const NAV = [
  { id: "buttons", label: "Buttons" },
  { id: "chips", label: "Badge · Tag · Kbd" },
  { id: "forms", label: "Form controls" },
  { id: "nav", label: "Tab · TreeRow · Menu" },
  { id: "feedback", label: "StatusDot · Spinner · Toast" },
  { id: "centerstate", label: "CenterState" },
  { id: "helphint", label: "HelpHint · Tooltip" },
  { id: "text", label: "Hint text" },
  { id: "data", label: "Table" },
  { id: "listctrl", label: "ListCtrl" },
  { id: "scrollbar", label: "Scrollbar" },
];

const ic = {
  plus: <GIcon d={<path d="M12 5v14M5 12h14" />} />,
  term: <GIcon d={<><rect x="3" y="4" width="18" height="16" rx="2" /><path d="m7 9 3 3-3 3M13 15h4" /></>} />,
  md: <GIcon d={<><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M7 15V9l2.5 3L12 9v6M16 9v4m0 0 2-2m-2 2-2-2" /></>} />,
  search: <GIcon d={<><circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" /></>} />,
  folder: <GIcon d={<path d="M4 20h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-7l-2-2H4a1 1 0 0 0-1 1v13a1 1 0 0 0 1 1z" />} />,
  file: <GIcon d={<><path d="M14 3v4a1 1 0 0 0 1 1h4" /><path d="M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2z" /></>} />,
  folderOpen: <GIcon d={<path d="M3 8a1 1 0 0 1 1-1h5l2 2h7a1 1 0 0 1 1 1v1H3z M3 11h18l-1.5 8a1 1 0 0 1-1 1H5.5a1 1 0 0 1-1-1z" />} />,
  split: <GIcon d={<><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M12 4v16" /></>} />,
  copy: <GIcon d={<><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V5a2 2 0 0 1 2-2h8" /></>} />,
  trash: <GIcon d={<path d="M4 7h16M10 11v6M14 11v6M5 7l1 13a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1l1-13M9 7V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v3" />} />,
};

// AutoComplete specimen candidate lists (path-shaped — recent dirs / files).
const AC_DIRS = ["~/Downloads", "~/work/tasty", "~/work/tasty-ui/src", "~/work/tasty/crates/tasty-ui-widgets", "~/.config/tasty"];
const AC_FILES = ["~/work/tasty/README.md", "~/work/tasty/docs/architecture.md", "~/work/tasty/docs/design/systems/theme.md", "~/work/tasty/CHANGELOG.md"];
const AC_MANY = ["~/work/tasty", "~/work/tasty-ui/src", "~/work/tasty/crates/tasty-core", "~/work/tasty/crates/tasty-ui-widgets", "~/work/tasty/crates/tasty-gallery", "~/work/tasty/docs/adr", "~/work/tasty/docs/design/policies", "~/.config/tasty", "~/.local/share/tasty", "~/Downloads/exports"];

// MultiSelect specimen data — the DAG list status filter (first consumer).
const MS_STATES = [
  { value: "waiting", label: "Waiting" }, { value: "ready", label: "Ready" },
  { value: "running", label: "Running" }, { value: "done", label: "Done" },
  { value: "failed", label: "Failed" }, { value: "cancelled", label: "Cancelled" },
];
const MS_STATES_DISABLED = MS_STATES.map((o) => (o.value === "cancelled" ? { ...o, disabled: true } : o));
const MS_LONG = [
  { value: "a", label: "release-candidate-nightly-build-pipeline" },
  { value: "b", label: "integration-tests-postgres-and-redis" },
  { value: "c", label: "docs" },
];
const MS_MANY = Array.from({ length: 20 }, (_, i) => ({
  value: "opt-" + String(i + 1).padStart(2, "0"),
  label: "Workspace " + String(i + 1).padStart(2, "0"),
}));
// The injected i18n hook: 0 → null (falls back to placeholder) · N · all.
const msSummary = (n, total) => (n === 0 ? null : n === total ? "All" : n + " selected");

function Components() {
  return (
    <>
      {/* BUTTONS */}
      <Section id="buttons" title="Buttons">
        <Spec id="button" title="Button" badges={<HoverBadge />}
          when={<>The text action. <b>primary</b> for the one affirmative action, <b>secondary</b> (outlined) for alternates, <b>ghost</b> for low-emphasis/toolbar, <b>danger</b> for destructive, <b>agent</b> (mauve) when an AI agent is the actor. One primary per view.</>}>
          <Stage variant="column" style={{ gap: 18 }}>
            <Cluster label="variants — hover & click them">
              <Button variant="primary">Save</Button>
              <Button variant="secondary">Open folder</Button>
              <Button variant="ghost">Cancel</Button>
              <Button variant="danger">Force detach</Button>
              <Button variant="agent">Run agent task</Button>
            </Cluster>
            <Cluster label="sizes — sm 24 · md 28 · lg 32">
              <Button size="sm" variant="secondary">Small</Button>
              <Button size="md" variant="secondary">Medium</Button>
              <Button size="lg" variant="secondary">Large</Button>
            </Cluster>
            <Cluster label="with icons · disabled">
              <Button variant="secondary" leadingIcon={ic.plus}>New tab</Button>
              <Button variant="primary" trailingIcon={ic.search}>Search</Button>
              <Button variant="secondary" disabled>Disabled</Button>
            </Cluster>
            <Cluster label={'ghost ground="raised" — on surface-raised the label takes button-ghost-raised-fg → text-secondary-raised (Latte 5.17:1, was 4.05) (2026-10-21)'}>
              {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
                <div key={label} {...(t ? { "data-theme": t } : {})} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius)" }}>
                  <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary-raised)" }}>{label}</span>
                  <Button variant="ghost" size="sm" ground="raised">Cancel (panel)</Button>
                  <Button variant="ghost" size="sm" ground="raised">Cancel (raised)</Button>
                </div>
              ))}
            </Cluster>
          </Stage>
          <Meta
            specs={[["height", "28 / 24 / 32px"], ["padding", <>0 <span className="tok">--tasty-space-md</span></>], ["radius", <>4px <span className="tok">--tasty-radius</span></>], ["hover / active", "8% / 12% overlay"], ["focus", "2px accent ring"]]}
            tokens={[
              { tok: "--tasty-accent-primary", use: "primary fill", color: "var(--tasty-accent-primary-raised)" },
              { tok: "--tasty-accent-danger", use: "danger fill", color: "var(--tasty-accent-danger-raised)" },
              { tok: "--tasty-accent-agent", use: "agent fill", color: "var(--tasty-accent-agent-raised)" },
              { tok: "--tasty-overlay-hover", use: "8% hover" },
              { tok: "--tasty-text-on-accent", use: "filled label" },
            ]} />
        </Spec>

        <Spec id="disabled-ink-never-opacity" title="Disabled — ink, never opacity"
          when={<>A disabled control is <b>not faded</b>. Every variant draws the same <b>neutral box</b> (<span className="tok">--tasty-button-disabled-bg</span> / <span className="tok">-border</span>) and its label and icons take the one disabled ink (<span className="tok">--tasty-button-disabled-fg</span> → text-disabled). Accent fills (primary · agent · danger) <b>drop out</b>, so the disabled ink never sits on an accent. Ghost keeps no box. No hover or active overlay; cursor stays default. The same rule runs through IconButton, Input, Select, MultiSelect, Checkbox, Switch, MenuItem and ListCtrl.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-md)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "grid", gridTemplateColumns: "auto auto auto", gap: "var(--tasty-space-sm) var(--tasty-space-md)", alignItems: "center", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>enabled</span>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>disabled</span>
                {["primary", "agent", "danger", "secondary", "ghost"].map((v) => (
                  <React.Fragment key={v}>
                    <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{v}</span>
                    <span style={{ display: "flex" }}><Button variant={v} leadingIcon={ic.plus}>New tab</Button></span>
                    <span style={{ display: "flex" }}><Button variant={v} leadingIcon={ic.plus} disabled>New tab</Button></span>
                  </React.Fragment>
                ))}
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>controls</span>
                <span style={{ display: "flex", gap: "var(--tasty-space-sm)", alignItems: "center" }}><IconButton aria-label="Search">{ic.search}</IconButton><Checkbox defaultChecked label="Wrap" /><Switch defaultChecked /></span>
                <span style={{ display: "flex", gap: "var(--tasty-space-sm)", alignItems: "center" }}><IconButton disabled aria-label="Search">{ic.search}</IconButton><Checkbox defaultChecked disabled label="Wrap" /><Switch defaultChecked disabled /></span>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["fill / edge", "surface-raised / border-default — every variant (ghost: none)"], ["label + icons", "text-disabled (one ink)"], ["accent fill", "drops out when disabled"], ["opacity", "none on disabled controls"], ["hover / active", "not drawn"], ["cursor", "default"], ["dimmed items", "switched-off rows · pending cut · inert regions → state-dim-opacity (0.5), not this rule"]]}
            tokens={[{ tok: "--tasty-button-disabled-bg", use: "→ state-disabled-fill", color: "var(--tasty-button-disabled-bg)" }, { tok: "--tasty-button-disabled-border", use: "→ state-disabled-border", color: "var(--tasty-button-disabled-border)" }, { tok: "--tasty-button-disabled-fg", use: "→ state-disabled-fg", color: "var(--tasty-button-disabled-fg)" }, { tok: "--tasty-state-dim-opacity", use: "items only" }]} />
          <Dont><b>Don't</b> multiply a disabled control by 0.5. Its label lands at a different step for every variant, and a faded accent fill still reads as "the primary action".</Dont>
        </Spec>

        <Spec id="iconbutton" title="IconButton"
          when={<>Square, icon-only — toolbars, tab close, sidebar rails. Same heights as Button. <b>ghost</b> by default; <b>active</b> shows the persistent accent selection (e.g. an engaged rail tool).</>}>
          <Stage>
            <Cluster label="ghost · solid · active">
              <IconButton aria-label="Split">{ic.split}</IconButton>
              <IconButton variant="solid" aria-label="Search">{ic.search}</IconButton>
              <IconButton active aria-label="Terminal">{ic.term}</IconButton>
              <IconButton size="sm" aria-label="New">{ic.plus}</IconButton>
            </Cluster>
          </Stage>
          <Meta
            specs={[["size", "28px (sm 24)"], ["shape", "square, 4px radius"], ["active", <span className="tok">--tasty-accent-primary</span>]]}
            tokens={[{ tok: "--tasty-overlay-hover", use: "hover tint" }, { tok: "--tasty-accent-primary", use: "active", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-text-muted", use: "rest icon", color: "var(--tasty-text-muted)" }]} />
        </Spec>
      </Section>

      {/* CHIPS */}
      <Section id="chips" title="Badge · Tag · Kbd">
        <Spec id="badge" title="Badge"
          when={<>Compact count or status pill. <b>danger</b> by default for unread counts (<code>99+</code>); use <code>dot</code> for a bare presence indicator on a rail icon. Every variant is a <b>bg/fg token pair</b> (<span className="tok">--tasty-badge-&lt;variant&gt;-bg/-fg</span>) — a badge never reaches into an accent role directly. <b>primary</b> (blue) = Completion attention, <b>warning</b> (yellow) = NeedsInput attention; when a row shows both, wrap them in <code>BadgeGroup</code> — NeedsInput leads, Completion trails.</>}>
          <Stage>
            <Cluster label="counts"><Badge>3</Badge><Badge>99+</Badge><Badge variant="primary">12</Badge><Badge variant="warning">2</Badge><Badge variant="agent">new</Badge><Badge variant="success">ok</Badge></Cluster>
            <Cluster label="attention kinds"><BadgeGroup><Badge variant="warning">2</Badge><Badge variant="primary">5</Badge></BadgeGroup><BadgeGroup><Badge variant="warning">99+</Badge><Badge variant="primary">99+</Badge></BadgeGroup></Cluster>
            <Cluster label="dot"><Badge dot variant="danger" /><Badge dot variant="warning" /><Badge dot variant="primary" /><Badge dot variant="agent" /><Badge dot variant="success" /></Cluster>
          </Stage>
          <Meta specs={[["radius", <span className="tok">--tasty-radius-pill</span>], ["size", "caption 11px"], ["group gap", <span className="tok">--tasty-badge-group-gap</span>], ["overflow", "99+"]]}
            tokens={[{ tok: "--tasty-badge-danger-bg", use: "default", color: "var(--tasty-badge-danger-bg)" }, { tok: "--tasty-badge-warning-bg", use: "NeedsInput count", color: "var(--tasty-badge-warning-bg)" }, { tok: "--tasty-badge-primary-bg", use: "Completion count", color: "var(--tasty-badge-primary-bg)" }, { tok: "--tasty-badge-agent-bg", use: "agent", color: "var(--tasty-badge-agent-bg)" }, { tok: "--tasty-font-size-caption", use: "11px" }]} />
          <Note>Attention kinds and their rank live on <b>Layouts › Attention kinds</b>; this spec only shows the chip. Don't pick <b>warning</b>/<b>primary</b> for a non-attention count — the two colors are read as “blocked” and “finished” everywhere else in the product.</Note>
        </Spec>

        <Spec id="tag" title="Tag"
          when={<>Small monospace label for <b>surface kinds</b> (<code>terminal</code>, <code>markdown</code>), permissions (<code>fs:read</code>), plugin origins, and IDs. Outlined by default; the <b>agent</b> variant marks agent-owned things.</>}>
          <Stage>
            <Cluster label="variants">
              <Tag>terminal</Tag><Tag variant="accent">markdown</Tag><Tag variant="agent">plugin</Tag>
              <Tag variant="success" dot>running</Tag><Tag variant="warning" dot>readonly</Tag><Tag variant="danger" dot>error</Tag>
              <Tag variant="info" dot>info</Tag>
            </Cluster>
          </Stage>
          <Meta specs={[["font", <span className="tok">--tasty-font-mono</span>], ["radius", <span className="tok">--tasty-radius-sm</span>], ["dot", "leading state dot"]]}
            tokens={[{ tok: "--tasty-font-mono", use: "label" }, { tok: "--tasty-border-default", use: "outline" }, { tok: "--tasty-accent-agent", use: "agent", color: "var(--tasty-accent-agent-raised)" }]} />
        </Spec>

        <Spec id="kbd" title="Kbd"
          when={<>Keyboard shortcuts as keycaps, in settings, menus, and the command palette. Pass an array or a <code>"+"</code>-joined string; it splits and caps each key.</>}>
          <Stage>
            <Cluster label="shortcuts"><Kbd keys="Ctrl+K" /><Kbd keys="Ctrl+Shift+N" /><Kbd keys={["⌘", ","]} /><Kbd keys="Esc" /></Cluster>
          </Stage>
          <Meta tokens={[{ tok: "--tasty-font-mono", use: "keycap" }, { tok: "--tasty-surface-raised", use: "cap fill", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-default", use: "cap border" }]} />
        <Spec id="one-keycap-no-palette-variant" title="One keycap, no palette variant"
          when={<>The command palette drew its own keycaps — <b>18</b> square, <b>5</b> side padding, <b>4</b> gap, <b>11</b>px type — against <b>Kbd</b>'s 16 / 4 / 3 / 10. That is drift, not a variant: nothing about a palette row asks for a bigger key, and a second size would have to be maintained in two places. The palette <b>converges on Kbd</b>. In a <b>28px</b> palette row a 16px cap leaves 6px of air above and below and still centres on the row's text baseline, so no row height or alignment changes.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 14, alignItems: "flex-start" }}>
            {[["settled — Kbd (16 / 4 / 3 / 10)", true], ["dropped — palette-only cap (18 / 5 / 4 / 11)", false]].map(([label, canonical]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6, width: 380 }}>
                <div style={{ fontSize: 11, color: canonical ? "var(--tasty-accent-success-raised)" : "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", padding: 4 }}>
                  {[["Split pane right", "Ctrl+Shift+D"], ["Open file…", "Ctrl+P"]].map(([cmd, keys]) => (
                    <div key={cmd} style={{ height: 28, display: "flex", alignItems: "center", gap: 8, padding: "0 8px", borderRadius: "var(--tasty-radius-sm)" }}>
                      <span style={{ flex: 1, fontSize: 13, color: "var(--tasty-text-secondary)" }}>{cmd}</span>
                      {canonical
                        ? <Kbd keys={keys} />
                        : <span style={{ display: "inline-flex", gap: 4 }}>{keys.split("+").map((k) => (
                            <span key={k} style={{ minWidth: 18, height: 18, padding: "0 5px", display: "inline-flex", alignItems: "center", justifyContent: "center",
                              fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-kbd-fg)", background: "var(--tasty-kbd-bg)",
                              border: "1px solid var(--tasty-kbd-border)", borderRadius: "var(--tasty-radius-sm)",
                              boxShadow: "0 var(--tasty-kbd-shadow-depth) 0 var(--tasty-kbd-border)" }}>{k}</span>))}
                          </span>}
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["min side", <>16 — <span className="tok">--tasty-kbd-size</span> (was 18)</>], ["padding-x", <>4 — <span className="tok">--tasty-kbd-padding-x</span> (was 5)</>], ["gap", <>3 — <span className="tok">--tasty-kbd-gap</span> (was 4)</>], ["font", <>10 — <span className="tok">--tasty-kbd-font-size</span> (was 11)</>], ["bottom edge", <>unchanged — <span className="tok">--tasty-kbd-shadow-depth</span></>], ["row", "28px palette row — unchanged, 6px air per side"], ["new tokens", "none"]]}
            tokens={[{ tok: "--tasty-kbd-size", use: "cap min side" }, { tok: "--tasty-kbd-padding-x", use: "cap side padding" }, { tok: "--tasty-kbd-gap", use: "between caps" }, { tok: "--tasty-kbd-font-size", use: "cap label" }]} />
          <Note>The switch-number overlay keycap already reads the Kbd tokens, so the palette was the last divergent cap. One keycap in the system now.</Note>
        </Spec>

        <Spec id="glyph-sizes-are-icons-not-type" title="Glyph sizes are icons, not type"
          when={<>Four values were sitting in the font scale because they were written as font sizes: the <b>toggle check</b> (12), the <b>spinner</b> and the <b>switch-overlay keycap digit</b> (16), and the <b>clipboard image glyph</b> (30). Three of them are <b>glyphs</b>, so they are judged by the icon family and land on it exactly — <b>12 = </b><span className="tok">--tasty-icon-size-xs</span>, <b>16 = </b><span className="tok">--tasty-icon-size-md</span>. The clipboard glyph is a <b>content</b> mark and snaps to the sanctioned <b>28</b> exception it sits next to, dropping the 30. No font token is created for any of them.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 26, flexWrap: "wrap" }}>
            {[["toggle check", "var(--tasty-icon-size-xs)", "check"], ["spinner", "var(--tasty-icon-size-md)", null], ["clipboard glyph", "28px", "clipboard"]].map(([label, size, icon]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
                <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", height: 34, color: "var(--tasty-text-secondary)" }}>
                  {icon ? <GIcon d={icon === "check" ? <path d="m5 13 4 4L19 7" /> : <><rect x="8" y="3" width="8" height="4" rx="1" /><path d="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2" /></>} size={icon === "check" ? 12 : 28} /> : <Spinner size={16} />}
                </span>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{label} · {size.replace("var(--tasty-", "").replace(")", "")}</span>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["toggle check", <>12 → <span className="tok">--tasty-icon-size-xs</span></>], ["spinner default", <>16 → <span className="tok">--tasty-icon-size-md</span></>], ["switch-overlay digit", <>16 — stays on <span className="tok">--tasty-kbd-size</span> (a keycap, not an icon)</>], ["clipboard glyph", "30 → 28 (the existing content-glyph exception)"], ["central glyph 22 / empty glyph 26", "unchanged named exceptions"], ["new font tokens", "none"]]}
            tokens={[{ tok: "--tasty-icon-size-xs", use: "12 — inline glyphs" }, { tok: "--tasty-icon-size-md", use: "16 — toolbar / spinner" }, { tok: "--tasty-spinner-size", use: "→ icon-size-md" }]} />
          <Note>Font <b>sizes</b> and glyph <b>sizes</b> are separate families. A 16px glyph next to 13px text is not a type-scale violation; it is an icon at its own size.</Note>
        </Spec>
        </Spec>
      </Section>

      {/* FORMS */}
      <Section id="forms" title="Form controls">
        <Spec id="input" title="Input" badges={<HoverBadge label="focus me" />}
          when={<>Single-line field. Use <code>mono</code> for paths, IDs, regex, hex. <code>icon</code> for a leading affordance (search), <code>addon</code> for a trailing unit. <code>invalid</code> turns the border + ring red. <code>readOnly</code> (2026-09-29) is for a value that can't be edited <b>now</b> but is still read: the disabled neutral box, the value in <span className="tok">--tasty-input-readonly-fg</span> (text-secondary-raised), focusable, selectable and copyable. <code>disabled</code> means the control itself is unavailable.</>}>
          <Stage variant="column" style={{ gap: 12 }}>
            <Cluster label="default · icon · addon — click to focus">
              <Input placeholder="Workspace name" style={{ width: 200 }} />
              <Input icon={ic.search} placeholder="Filter…" style={{ width: 200 }} />
              <Input mono defaultValue="14" addon="px" style={{ width: "var(--tasty-field-width-color)" }} />
            </Cluster>
            <Cluster label="mono · invalid · disabled">
              <Input mono defaultValue="s_01HXK9" style={{ width: 200 }} />
              <Input invalid defaultValue="bad value" style={{ width: 160 }} />
              <Input disabled placeholder="Disabled" style={{ width: 160 }} />
            </Cluster>
            <Cluster label="readOnly vs disabled — same box, readable value (2026-09-29)">
              <Input mono readOnly defaultValue="#89b4fa" style={{ width: "var(--tasty-field-width-xs)" }} />
              <Input mono disabled defaultValue="#89b4fa" style={{ width: "var(--tasty-field-width-xs)" }} />
            </Cluster>
            <Cluster label={'ground="raised" — on surface-raised, box steps down to bg-panel + border-strong (2026-10-14)'}>
              <div style={{ display: "flex", gap: 12, padding: 12, background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius)" }}>
                <Input ground="raised" placeholder="Editable" style={{ width: 140 }} />
                <Input ground="raised" mono readOnly defaultValue="#89b4fa" style={{ width: "var(--tasty-field-width-xs)" }} />
                <Input ground="raised" mono disabled defaultValue="#89b4fa" style={{ width: "var(--tasty-field-width-xs)" }} />
              </div>
            </Cluster>
          </Stage>
          <Meta
            specs={[["height", <>28px <span className="tok">--tasty-control-height</span></>], ["border", "1px → focus ring 2px"], ["padding", <>0 <span className="tok">--tasty-space-sm</span></>]]}
            tokens={[{ tok: "--tasty-surface-raised", use: "fill", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-focus", use: "ring", color: "var(--tasty-border-focus)" }, { tok: "--tasty-accent-danger", use: "invalid", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-text-placeholder", use: "hint" }, { tok: "--tasty-input-readonly-fg", use: "read-only value → text-secondary-raised", color: "var(--tasty-input-readonly-fg)" }]} />
        </Spec>

        <Spec id="select-checkbox-switch" title="Select · Checkbox · Switch"
          when={<><b>Select</b> = native dropdown, styled to match Input. <b>Checkbox</b> for a standalone boolean in a list of settings (inside <b>MultiSelect</b> it is the option row). <b>Switch</b> for a single boolean setting that applies immediately.</>}>
          <Stage style={{ gap: 28 }}>
            <Cluster label="Select"><Select options={["Default (full rc)", "Tasty rc", "Custom"]} style={{ width: 180 }} /></Cluster>
            <Cluster label="Checkbox">
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <Checkbox label="Confirm on close" defaultChecked />
                <Checkbox label="Restore layout" />
              </div>
            </Cluster>
            <Cluster label="Switch">
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <Switch label="Ligatures" defaultChecked />
                <Switch label="Reduced motion" />
              </div>
            </Cluster>
          </Stage>
          <Stage variant="column" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-md)" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
              <div key={label} {...(t ? { "data-theme": t } : {})} style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-md)", alignItems: "flex-end", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ width: "100%", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} — Select states (2026-10-21): hover / focus are pinned with the same border values the CSS applies</span>
                {[["value", { defaultValue: "Tasty rc" }], ["hover", { defaultValue: "Tasty rc", style: { borderColor: "var(--tasty-border-strong)" } }],
                  ["focus / open", { defaultValue: "Tasty rc", style: { borderColor: "var(--tasty-border-focus)", boxShadow: "0 0 0 var(--tasty-border-width) var(--tasty-border-focus)" } }],
                  ["no value → select-placeholder", { placeholder: "Select a profile…" }], ["disabled + value → state-disabled-fg", { disabled: true, defaultValue: "Tasty rc" }],
                  ["disabled + no value → state-disabled-fg", { disabled: true, placeholder: "Select a profile…" }]].map(([k, p]) => (
                  <div key={k} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: "var(--tasty-field-width-lg)" }}>
                    <span style={{ fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{k}</span>
                    <Select block options={["Default (full rc)", "Tasty rc", "Custom"]} {...p} />
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Stage variant="column" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-md)" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
              <div key={label} {...(t ? { "data-theme": t } : {})} style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-md)", alignItems: "flex-end", padding: "var(--tasty-space-md)", background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ width: "100%", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary-raised)" }}>{label} — Select ground="raised" (2026-10-21): box bg-panel + border-strong in every state; ink and focus unchanged</span>
                {[["value", { defaultValue: "Tasty rc" }], ["focus / open", { defaultValue: "Tasty rc", style: { borderColor: "var(--tasty-border-focus)", boxShadow: "0 0 0 var(--tasty-border-width) var(--tasty-border-focus)" } }],
                  ["no value", { placeholder: "Select a profile…" }], ["disabled", { disabled: true, defaultValue: "Tasty rc" }], ["panel ground (vanishes)", { defaultValue: "Tasty rc", ground: "panel" }]].map(([k, p]) => (
                  <div key={k} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: "var(--tasty-field-width-lg)" }}>
                    <span style={{ fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary-raised)" }}>{k}</span>
                    <Select block ground="raised" options={["Default (full rc)", "Tasty rc", "Custom"]} {...p} />
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["control height", "28px"], ["checkbox", "16px square"], ["accent", <span className="tok">--tasty-accent-primary</span>],
              ["Select ink (2026-10-21)", "value = text-primary · no value = select-placeholder · disabled (value or not) = state-disabled-fg, chevron state-disabled-fg (else text-muted) · open list options always text-primary · no hover/active overlay when disabled"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "checked / on", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-surface-raised", use: "track / box", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-default", use: "outline" }]} />
        </Spec>

        <Spec id="multiselect-many-values-in-one-control" title="MultiSelect — many values in one control" badges={<HoverBadge label="click me" />}
          when={<>The <b>Select sibling</b> (not a variant) for when a control holds <b>more than one value</b> — an OR filter like <span className="ic">waiting + ready + running</span>. Closed it is a <code>Select</code>: same 28px height, border, radius, font, chevron, so the two read as one family in a form. Open it is a <b>menu of <code>Checkbox</code> rows that stays open while you toggle</b> — outside click or <span className="ic">Esc</span> closes. The trigger never enumerates the selection; it shows a <b>caller-injected 3-branch summary</b> (0 → placeholder tone · N → <span className="ic">3 selected</span> · all → <span className="ic">All</span>) as <b>plain text</b>, never a <code>Badge</code>. Checked rows get <b>no background</b> — the checkmark is the only checked signal; backgrounds are reserved for pointer <b>hover</b> and the keyboard <span className="ic">↑↓</span> <b>active</b> row.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 22 }}>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 240px)", gap: 20, justifyContent: "center" }}>
              {[
                ["live — click to open, toggle freely", <MultiSelect key="l" block options={MS_STATES} defaultValue={["waiting", "ready", "running"]} summary={msSummary} placeholder="No status" />, 44],
                ["summary — 0 selected (placeholder tone)", <MultiSelect key="s0" block options={MS_STATES} value={[]} summary={msSummary} placeholder="No status" />, 44],
                ["summary — all selected", <MultiSelect key="sa" block options={MS_STATES} value={MS_STATES.map(o => o.value)} summary={msSummary} />, 44],
                ["focus / open — border-focus + 1px ring, chevron flipped", <MultiSelect key="o" block open options={MS_STATES} value={["waiting", "ready", "running"]} summary={msSummary} />, 230],
                ["rows — hover (row 4) vs keyboard-active (row 1)", <MultiSelect key="h" block open activeIndex={0} hoverIndex={3} options={MS_STATES} value={["waiting", "ready", "running"]} summary={msSummary} />, 230],
                ["per-option disabled (row 6: no cancelled runs)", <MultiSelect key="d" block open activeIndex={-1} options={MS_STATES_DISABLED} value={["running"]} summary={msSummary} />, 230],
                ["disabled control", <MultiSelect key="dd" block disabled options={MS_STATES} value={["running"]} summary={msSummary} />, 44],
                ["long labels — ellipsis in trigger and rows", <MultiSelect key="e" block open activeIndex={-1} options={MS_LONG} value={["a", "b"]} summary={(n) => n + " pipelines selected — very long summary"} />, 200],
                ["20 options → internal scroll at 220", <MultiSelect key="m" block open activeIndex={-1} options={MS_MANY} value={["opt-02", "opt-05"]} summary={msSummary} allToggle />, 230],
              ].map(([label, node, minH]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: 8, minHeight: minH, alignItems: "stretch" }}>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                  <div style={{ position: "relative" }}>{node}</div>
                </div>
              ))}
            </div>
            <div style={{ display: "flex", gap: 16, alignItems: "flex-end", padding: "14px 16px", background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: 4 }}>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Sort by — Select (one value)</div>
                <Select options={["Started", "Name", "Duration"]} style={{ width: 160 }} />
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Status — MultiSelect (many)</div>
                <MultiSelect options={MS_STATES} defaultValue={["waiting", "ready", "running"]} summary={msSummary} placeholder="No status" style={{ width: 160 }} />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["trigger", <>Select language — 28px <span className="tok">--tasty-multiselect-height</span> · 12 left · 28 chevron room</>], ["open trigger", "border-focus + 1px ring, chevron rotates 180°"], ["summary", "3 branches, caller-injected · plain text · single line + ellipsis"], ["menu", <>4 below the trigger · min-width = trigger · grows to content up to <span className="tok">--tasty-multiselect-menu-max-width</span> (320)</>], ["row", "28px · 12 padding-x · 8 box→label · Checkbox 16px"], ["checked row", "checkmark only — no background"], ["overflow", <>scrolls past 220 <span className="tok">--tasty-multiselect-menu-max-height</span> · <code>.tasty-scroll</code></>], ["bulk row", <><span className="tok">allToggle</span> — accent action row + separator, off below ~8 options</>], ["keys", "↓/↵/Space open · ↑↓ Home End move · Space/↵ toggle (menu stays) · Esc close"], ["motion", "border/chevron 120ms · check state 0ms"]]}
            tokens={[
              { tok: "--tasty-multiselect-bg", use: "trigger fill", color: "var(--tasty-multiselect-bg)" },
              { tok: "--tasty-multiselect-border", use: "trigger edge", color: "var(--tasty-multiselect-border)" },
              { tok: "--tasty-multiselect-border-focus", use: "focus / open", color: "var(--tasty-multiselect-border-focus)" },
              { tok: "--tasty-multiselect-summary-fg-empty", use: "0 selected", color: "var(--tasty-multiselect-summary-fg-empty)" },
              { tok: "--tasty-multiselect-menu-bg", use: "menu fill", color: "var(--tasty-multiselect-menu-bg)" },
              { tok: "--tasty-multiselect-row-bg-hover", use: "pointer hover", color: "var(--tasty-multiselect-row-bg-hover)" },
              { tok: "--tasty-multiselect-row-bg-active", use: "keyboard-active", color: "var(--tasty-multiselect-row-bg-active)" },
              { tok: "--tasty-multiselect-row-fg", use: "row label (primary, not muted)", color: "var(--tasty-multiselect-row-fg)" },
              { tok: "--tasty-checkbox-bg-checked", use: "checked box", color: "var(--tasty-checkbox-bg-checked)" },
              { tok: "--tasty-multiselect-all-fg", use: "bulk select/clear", color: "var(--tasty-multiselect-all-fg)" },
            ]} />
          <Note><b>Decisions (implementation spec).</b> <b>Sibling, not a <code>Select</code> variant</b> — the open surface is a different thing (checkbox menu, non-native, stays open), so it earns its own catalog entry; <code>Select</code> is untouched. <b>Open = focus treatment + flipped chevron</b>, no third visual state. <b>Bulk toggle exists but is opt-in</b> (<span className="tok">allToggle</span>): one accent action row that reads "Select all" and becomes "Clear all" once everything is on — a checkbox there would need an indeterminate state this system doesn't have. <b>Menu may outgrow the trigger</b> up to 320, because the trigger holds a short summary while the rows hold real labels. <b>Per-option disabled is supported</b> (a status with no runs, a permission-gated option) — row keeps <span className="tok">--tasty-state-disabled-opacity</span> and is not togglable. <b>Count is plain text</b>, not a <code>Badge</code>: badges live on tabs and rails, not inside form controls. <b>Zero selected is allowed</b> — for a filter, nothing checked should mean <i>no filter</i> (show all) rather than an empty view; forcing a minimum is consumer policy, not a control rule. Row labels are <span className="tok">--tasty-text-primary</span> (never muted) so Latte stays ≥4.5:1 on <span className="tok">--tasty-surface-raised</span>. First consumer: the DAG list status filter (<code>ui_kits/terminal/overlays/dag_view.jsx</code>) — 6 statuses, <span className="tok">allToggle</span> off.</Note>
        </Spec>

        {GallerySearchableSelect && <GallerySearchableSelect />}

        <Spec id="autocomplete-free-text-trigger-candidate-dropdown-typeahead" title="AutoComplete — free-text trigger + candidate dropdown (typeahead)"
          when={<>A real <b>typeahead</b>: a free-text trigger (<code>Input</code> language) with a floating <b>candidate dropdown</b> (menu container + <code>MenuItem</code> language) that <b>narrows as you type</b>. Use it — not <b>Select</b> — when the user types free text and the list only <i>suggests</i> (paths, recent files, recent directories). <b>Select</b> stays the closed picker for a fixed value set. Path candidates render <b>middle-ellipsis</b> (filename tail preserved); the matched run is highlighted in <span className="tok">--tasty-accent-primary</span>. Two row states: pointer <b>hover</b> (<span className="tok">--tasty-overlay-hover</span>) vs keyboard <span className="ic">↑↓</span> <b>active</b> (<span className="tok">--tasty-surface-active</span>, stronger — wins when both). <b><span className="tok">maxDropdownHeight</span></b> caps the list; past it the list <b>scrolls internally</b> (<code>.tasty-scroll</code>) and shrinks-to-fit below.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 22 }}>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 260px)", gap: 20, justifyContent: "center" }}>
              {[
                ["idle — closed trigger", <AutoComplete key="i" block mono withGo icon={ic.folderOpen} rowIcon={ic.folderOpen} open={false} query="~/work/tasty" items={AC_DIRS} />, 44],
                ["open — full candidate list", <AutoComplete key="o" block mono withGo icon={ic.folderOpen} rowIcon={ic.folderOpen} open activeIndex={0} query="" items={AC_DIRS} maxDropdownHeight={220} />, 200],
                ["typing → filtered + highlight", <AutoComplete key="f" block mono withGo icon={ic.folderOpen} rowIcon={ic.folderOpen} open activeIndex={0} query="tasty" items={AC_DIRS} maxDropdownHeight={220} />, 160],
                ["overflow → internal scroll", <AutoComplete key="s" block mono withGo icon={ic.folderOpen} rowIcon={ic.folderOpen} open activeIndex={0} query="" items={AC_MANY} maxDropdownHeight={132} />, 170],
                ["empty / no match", <AutoComplete key="e" block mono withGo icon={ic.folderOpen} rowIcon={ic.folderOpen} open query="zzzz" items={AC_DIRS} emptyLabel="No matching path" />, 90],
                ["hover (row 3) vs keyboard-active (row 1)", <AutoComplete key="h" block mono withGo icon={ic.folderOpen} rowIcon={ic.folderOpen} open activeIndex={1} hoverIndex={3} query="" items={AC_DIRS} maxDropdownHeight={220} />, 200],
              ].map(([label, node, minH]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: 8, minHeight: minH, alignItems: "stretch" }}>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                  <div style={{ position: "relative" }}>{node}</div>
                </div>
              ))}
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, 260px)", gap: 20, justifyContent: "center" }}>
              {[
                ["explorer context — folderOpen · recent directories", ic.folderOpen, AC_DIRS],
                ["markdown context — file · recent files", ic.file, AC_FILES],
              ].map(([label, icon, items]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: 8, minHeight: 200 }}>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                  <div style={{ position: "relative" }}><AutoComplete block mono withGo icon={icon} rowIcon={icon} open activeIndex={0} query="" items={items} maxDropdownHeight={220} /></div>
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["trigger", "Input language · leading icon slot · mono · focus ring on open"], ["dropdown", "menu container + MenuItem rows · popover lift"], ["row height", <>28px <span className="tok">--tasty-control-height</span></>], ["path row", "middle-ellipsis (filename tail kept)"], ["filter", "substring (default) · prefix · none"], ["row states", "hover (overlay-hover) · keyboard-active (surface-active, wins)"], ["overflow", <>scroll past <span className="tok">maxDropdownHeight</span> · <code>.tasty-scroll</code></>], ["empty", "one muted no-match row"]]}
            tokens={[{ tok: "--tasty-input-bg", use: "trigger fill", color: "var(--tasty-input-bg)" }, { tok: "--tasty-input-border-focus", use: "trigger border (open)", color: "var(--tasty-input-border-focus)" }, { tok: "--tasty-autocomplete-menu-bg", use: "dropdown fill", color: "var(--tasty-autocomplete-menu-bg)" }, { tok: "--tasty-autocomplete-menu-border", use: "dropdown edge", color: "var(--tasty-autocomplete-menu-border)" }, { tok: "--tasty-autocomplete-row-bg-hover", use: "pointer hover", color: "var(--tasty-autocomplete-row-bg-hover)" }, { tok: "--tasty-autocomplete-row-bg-active", use: "keyboard-active", color: "var(--tasty-autocomplete-row-bg-active)" }, { tok: "--tasty-autocomplete-match-fg", use: "match highlight", color: "var(--tasty-autocomplete-match-fg)" }, { tok: "--tasty-autocomplete-max-height", use: "default cap (220)" }, { tok: "--tasty-autocomplete-empty-fg", use: "no-match row", color: "var(--tasty-autocomplete-empty-fg)" }]} />
          <Note>Replaces the source-only <code>Combobox</code> widget (<code>crates/tasty-ui-widgets/src/combobox.rs</code>), which implied a <b>closed</b> selection it never was — its one real job is free-text + suggestions = <b>autocomplete</b>. <b>Match:</b> substring is the recommended default (paths match anywhere, not just the prefix); the matched run is highlighted — v1 source list was unfiltered, this design formalizes typeahead. <b>Scrollbar:</b> reuses the shared <code>.tasty-scroll</code> (neutral-400 thumb) — <b>no new token</b>. This is the candidate dropdown behind the shared <b>PathField</b> (see Plugins → Explorer): Explorer injects <span className="ic">folderOpen</span> + recent directories, Markdown injects <span className="ic">file</span> + recent files; the component emits only the confirmed path string. Source home for the extracted widget: rename <code>Combobox</code> → <code>AutoComplete</code>, compose it under <code>PathField</code>, wire both call sites — tracked as separate implementation work.</Note>
        </Spec>
        <Spec id="text-caret-steady-never-blinks" title="Text caret — steady, never blinks"
          when={<>The caret in every UI text field (palette search, settings fields, popup and modal inputs, single- and multi-line) is <b>steady</b>: always drawn while the field has focus and the window is focused, never blinking. This matches the terminal cursor (motion 0 ms) and keeps an idle focused field from waking the frame loop twice a second. Focus is already carried by the field's focus border, so the caret does not need motion to be found. Window not focused → no caret, as today.</>}>
          <Stage variant="column" style={{ gap: 12, alignItems: "flex-start", maxWidth: 320 }}>
            <div style={{ position: "relative", width: 260, height: "var(--tasty-control-height)", display: "flex", alignItems: "center", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-input-bg, var(--tasty-surface-raised))", border: "var(--tasty-border-width) solid var(--tasty-border-focus)", borderRadius: "var(--tasty-radius)", fontSize: 13, color: "var(--tasty-text-primary)" }}>
              open settings<span style={{ display: "inline-block", width: "var(--tasty-border-width)", height: 16, background: "var(--tasty-text-primary)", marginLeft: 1 }}></span>
            </div>
          </Stage>
          <Meta
            specs={[["blink", "off · egui text_cursor.blink = false"], ["width", "1 · text-primary ink"], ["shown", "field focused AND window focused"], ["cost", "no idle repaint while a field is focused"]]}
            tokens={[{ tok: "--tasty-border-focus", use: "focus edge", color: "var(--tasty-border-focus)" }]} />
        </Spec>
      </Section>

      {/* NAV */}
      <Section id="nav" title="Tab · TreeRow · MenuItem">
        <Spec id="tab" title="Tab"
          when={<>One tab in the pane tab strip. <b>24px tall × 150px wide</b>, accent bar on the active tab, hover-revealed close, and a <b>busy dot</b> mirroring the product surface model (busy=green, idle=no dot). The title carries <b>attention</b>: <code>needs-input</code> (yellow) outranks <code>completion</code> (blue); the active tab never shows it (attention clears on focus). States are exactly the product's (2026-10-07): busy · needs-input · completion — no attached ring and no separate notif tint on a tab. The 5-color owner×activity vocabulary lives on the workspace StatusDot, not on tabs — Tasty has no "unsaved" state.</>}>
          <Stage variant="tight">
            <div style={{ display: "flex", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
              <Tab label="build.sh" icon={ic.term} active status="busy" />
              <Tab label="deploy.sh" icon={ic.term} attention="needs-input" />
              <Tab label="README.md" icon={ic.md} attention="completion" />
              <Tab label="server.log" icon={ic.term} />
            </div>
          </Stage>
          <Meta
            specs={[["height", <>24px <span className="tok">--tasty-control-height-tab</span></>], ["width", <>150px <span className="tok">--tasty-tab-width</span></>], ["active", "accent top bar + panel fill"], ["close", "hover-revealed"], ["title", "needs-input › completion › active › rest"], ["cluster", <>busy dot · close, 4 apart · <span className="tok">--tasty-tab-status-gap</span></>]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "active fill", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-accent-primary", use: "active bar", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-tab-fg-needs-input", use: "needs-input title", color: "var(--tasty-tab-fg-needs-input)" }, { tok: "--tasty-tab-fg-completion", use: "completion title", color: "var(--tasty-tab-fg-completion)" }, { tok: "--tasty-tab-status-gap", use: "→ space-xs 4" }, { tok: "--tasty-separator", use: "dividers" }]} />
        </Spec>

        <Spec id="treerow" title="TreeRow"
          when={<>A <b>22px</b> row for sidebars and file trees — indent per level (14px), disclosure chevron for folders, leading kind icon, label, trailing meta. The selected row gets <code>--tasty-surface-active</code>.</>}>
          <Stage variant="tight">
            <div style={{ background: "var(--tasty-bg-sidebar)", padding: "6px 4px", width: 320 }}>
              <TreeRow label="tasty" icon={ic.folder} expandable open level={0} meta="34" />
              <TreeRow label="crates" icon={ic.folder} expandable level={1} meta="39" />
              <TreeRow label="Cargo.toml" icon={ic.file} level={1} selected />
              <TreeRow label="README.md" icon={ic.file} level={1} meta="4.7k" />
            </div>
          </Stage>
          <Meta
            specs={[["height", <>22px <span className="tok">--tasty-control-height-tree</span></>], ["indent", "14px / level"], ["selected", <span className="tok">--tasty-surface-active</span>], ["disabled", "none — no such state (2026-09-29)"]]}
            tokens={[{ tok: "--tasty-surface-active", use: "selected", color: "var(--tasty-surface-active)" }, { tok: "--tasty-overlay-hover", use: "hover" }, { tok: "--tasty-text-muted", use: "meta + icon", color: "var(--tasty-text-muted)" }]} />
          <Note><b>No disabled state (2026-09-29).</b> A tree row names something that exists. A folder without read permission or a favourite on a dropped remote stays a normal row: selectable, expandable, and opening it reports the reason (CenterState error in the explorer body). Greying it would hide the one row the user needs to act on. The product's <code>enabled</code> argument is removed. A pending cut is a dimmed item (<span className="tok">--tasty-cut-pending-opacity</span>), not disabled.</Note>
        </Spec>

        <Spec id="menuitem" title="MenuItem"
          when={<>A <b>28px</b> row for context menus, the command palette, and tools menus — icon + label + trailing shortcut. <b>active</b> = keyboard-highlighted; <b>danger</b> for destructive; <code>separator</code> draws a thin rule.</>}>
          <Stage variant="tight">
            <div style={{ background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", padding: 6, width: 280 }}>
              <MenuItem label="New tab" icon={ic.term} shortcut={<Kbd keys="Ctrl+T" />} active />
              <MenuItem label="Split pane" icon={ic.split} shortcut={<Kbd keys="Ctrl+D" />} />
              <MenuItem label="Copy path" icon={ic.copy} />
              <MenuItem separator />
              <MenuItem label="Move to Trash" icon={ic.trash} danger />
            </div>
          </Stage>
          <Meta
            specs={[["height", <>28px <span className="tok">--tasty-control-height</span></>], ["label · rest", <span className="tok">--tasty-menu-item-fg</span>], ["label · hover/active", <span className="tok">--tasty-menu-item-fg-hover</span>], ["row pitch", "28 — rows flush, no gap (every menu incl. the … menu)"], ["active", <span className="tok">--tasty-surface-active</span>], ["danger", <span className="tok">--tasty-accent-danger</span>]]}
            tokens={[{ tok: "--tasty-menu-item-fg", use: "resting label → text-secondary-raised", color: "var(--tasty-menu-item-fg)" }, { tok: "--tasty-menu-item-fg-hover", use: "hover / active label → text-on-active", color: "var(--tasty-menu-item-fg-hover)" }, { tok: "--tasty-surface-active", use: "highlighted", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-danger", use: "destructive", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-text-muted", use: "shortcut", color: "var(--tasty-text-muted)" }]} />
          <Note><b>Label colour (2026-10-07).</b> The token is canonical: a resting row reads <code>--tasty-menu-item-fg</code> (text-secondary), hover and keyboard-active lift to <code>--tasty-menu-item-fg-hover</code> (text-primary) on top of the background change. The command palette is a list, not a menu, and keeps its own row colours.</Note>
          <Note><b>Latte contrast (2026-10-08).</b> <code>--tasty-menu-item-fg</code> now reads the semantic role <code>--tasty-text-secondary-raised</code>: Mocha = neutral-1000 (7.1:1 on surface-raised), Latte = neutral-1100 (5.17:1; neutral-1000 was 4.05:1). Hover and keyboard-active read <code>--tasty-menu-item-fg-hover</code> → <code>--tasty-text-on-active</code> since 2026-11-05 (Latte 6.36 on the hover fill, 5.39 on surface-active), because a menu row is read while it is hovered.</Note>
        </Spec>

        <Spec id="menuitem-selected-option" title="MenuItem — selected option"
          when={<>The current value inside an open <b>Select</b>-style list (settings dropdowns, egui ComboBox). <code>selected</code> = <b>text-primary ink + trailing accent check</b>, <b>no fill</b>. Fills keep their two jobs: pointer hover (<code>--tasty-menu-item-bg-hover</code>) and keyboard-active (<code>--tasty-surface-active</code>), so a selected row under the pointer shows both. The egui default selection fill (accent-blue block) is not used.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} data-theme={attr || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} — rest · selected · rest · keyboard-active</span>
                <div role="menu" style={{ width: "var(--tasty-field-width-lg)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-space-xs)", boxShadow: "var(--tasty-shadow-popover)" }}>
                  <MenuItem label="Ask" />
                  <MenuItem label="Minimize to background" selected />
                  <MenuItem label="Quit" />
                  <MenuItem label="Quit and save layout" active />
                </div>
              </div>
            ))}
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>No value yet (2026-10-09) — trigger in placeholder ink, list = real options only</span>
              <div style={{ width: "var(--tasty-field-width-lg)", height: "var(--tasty-control-height)", boxSizing: "border-box", display: "flex", alignItems: "center", padding: "0 var(--tasty-space-md)", border: "var(--tasty-border-width) solid var(--tasty-border-focus)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-field)", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-placeholder)" }}>Choose an action</div>
              <div role="menu" style={{ width: "var(--tasty-field-width-lg)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-space-xs)", boxShadow: "var(--tasty-shadow-popover)" }}>
                <MenuItem label="Ask" active />
                <MenuItem label="Minimize to background" />
                <MenuItem label="Quit" />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["selected ink", <span className="tok">--tasty-menu-item-selected-fg</span>], ["check", <><span className="ic">check</span> icon · <span className="tok">--tasty-menu-item-check-size</span> 14 · <span className="tok">--tasty-menu-item-check-fg</span> accent-primary · trailing, after any shortcut</>], ["fill", "none — hover / keyboard fills only"], ["a11y", <>role menuitemradio · aria-checked</>], ["no value (2026-10-09)", "the placeholder is NOT a row — the list holds real options only, none checked, keyboard-active starts on row 1; the trigger alone shows the placeholder (text-placeholder). A Select that must be clearable gets a real option for it (e.g. “None”), drawn like any other row."]]}
            tokens={[{ tok: "--tasty-menu-item-selected-fg", use: "selected label → text-primary", color: "var(--tasty-menu-item-selected-fg)" }, { tok: "--tasty-menu-item-check-fg", use: "check glyph", color: "var(--tasty-menu-item-check-fg)" }, { tok: "--tasty-text-secondary-raised", use: "resting label role (Latte → n1100)", color: "var(--tasty-text-secondary-raised)" }]} />
        </Spec>
      </Section>

      {/* FEEDBACK */}
      <Section id="feedback" title="StatusDot · Spinner · Toast">
        <Spec id="statusdot" title="StatusDot"
          when={<>Small state indicator for surfaces, panes, plugins. <b>agent</b> (mauve, pulsing) marks a live agent surface — the single most important signal in the product. <code>pulse</code> for live/running. <b>needs-input</b> and <b>completion</b> are <b>attention kinds</b>, not execution states — they share this one dot and win it by rank (needs-input › completion › running/agent/waiting › idle).</>}>
          <Stage style={{ gap: 24 }}>
            <Cluster label="states"><StatusDot status="running" pulse label="running" /><StatusDot status="agent" pulse label="agent" /><StatusDot status="waiting" label="waiting" /><StatusDot status="idle" label="idle" /><StatusDot status="error" label="error" /></Cluster>
            <Cluster label="attention kinds"><StatusDot status="needs-input" label="needs input" /><StatusDot status="completion" label="completion" /></Cluster>
          </Stage>
          <Meta
            specs={[["dot", "7px"], ["pulse", "expanding ring"], ["agent", <span className="tok">--tasty-accent-agent</span>], ["attention rank", "needs-input › completion › activity"]]}
            tokens={[{ tok: "--tasty-accent-success", use: "running", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-accent-agent", use: "agent", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-accent-warning", use: "waiting", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-accent-danger", use: "error", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-status-dot-needs-input", use: "needs input", color: "var(--tasty-status-dot-needs-input)" }, { tok: "--tasty-status-dot-completion", use: "completion", color: "var(--tasty-status-dot-completion)" }]} />
        </Spec>

        <Spec id="selected-row" title="Selected row — meaning glyphs, match run, Kbd +"
          when={<>A selected or keyboard-active row keeps what each glyph means. <b>Hue carries the meaning</b> (favorite, plugin, remote, status): the glyph keeps its hue through its own <code>-on-active</code> role. <b>Shape carries the meaning</b> (file kinds, the unregistered star): the glyph takes the row ink. A <b>Tag</b> keeps its own fill and does not change. A <b>match run</b> takes an underline, because no accent ink reaches 4.5:1 on this fill and still differs from the row ink.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <SelRowGrid label="Mocha" theme="mocha" />
            <SelRowGrid label="Latte" theme="latte" />
          </Stage>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <SelInkTable label="Mocha" theme="mocha" />
            <SelInkTable label="Latte" theme="latte" />
          </Stage>
          <Meta
            specs={[["hue glyph", "accent-<role>-on-active · 3:1 or better at rest on the row"], ["shape glyph", "text-on-active"], ["selection mark", "accent-primary-on-active, unchanged"], ["chip", "Tag recipe · never a tinted pill"], ["match run", "1px underline in the row ink · weight not used"], ["Kbd +", "kbd-plus-fg on resting grounds · kbd-plus-fg-active on the row"], ["hover over the row", "transient · text-on-active holds 3:1 · glyphs carry no target"], ["Latte muted", "n1000 · shares its ink with text-secondary"]]}
            tokens={[{ tok: "--tasty-accent-warning-on-active", use: "star, waiting, needs input", color: "var(--tasty-accent-warning-on-active)" }, { tok: "--tasty-accent-success-on-active", use: "running", color: "var(--tasty-accent-success-on-active)" }, { tok: "--tasty-accent-danger-on-active", use: "error", color: "var(--tasty-accent-danger-on-active)" }, { tok: "--tasty-accent-agent-on-active", use: "plugin, agent", color: "var(--tasty-accent-agent-on-active)" }, { tok: "--tasty-accent-remote-on-active", use: "mirror", color: "var(--tasty-accent-remote-on-active)" }, { tok: "--tasty-accent-attached-on-active", use: "attached ring", color: "var(--tasty-accent-attached-on-active)" }, { tok: "--tasty-tag-attached-fg", use: "in use Tag", color: "var(--tasty-tag-attached-fg)" }, { tok: "--tasty-kbd-plus-fg-active", use: "+ on the row", color: "var(--tasty-kbd-plus-fg-active)" }, { tok: "--tasty-explorer-match-fg-active", use: "match run on the row", color: "var(--tasty-explorer-match-fg-active)" }, { tok: "--tasty-text-muted", use: "Latte n1000", color: "var(--tasty-text-muted)" }]} />
          <Do><b>Do</b> re-point the dot tokens on the row. The dot draws the same way, and only its token changes.</Do>
          <Dont><b>Don't</b> lay an accent tint on a selected row. Over surface-active a 12–16% tint leaves Latte ink at 1.2–1.4:1, so a chip takes the Tag recipe.</Dont>
        </Spec>

        <Spec id="status-resolution-always-exactly-one-dot" title="Status resolution — always exactly one dot"
          when={<>A surface carries <b>two independent facts</b>: who <b>owns</b> it (user / agent) and what it's <b>doing</b> (running / waiting / error / idle). These never render as two dots — they collapse into <b>one</b> by a fixed priority. Live activity outranks ownership, so <b>agent + running shows running (green)</b>; the mauve <b>agent</b> dot appears only when the surface is otherwise idle.</>}>
          <Stage variant="solo column" style={{ gap: 0, padding: "4px 26px 10px" }}>
            <div style={{ display: "flex", gap: 10, padding: "8px 0", borderBottom: "1px solid var(--tasty-separator)",
              fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".05em", color: "var(--tasty-text-muted)" }}>
              <span style={{ width: 92 }}>owner</span><span style={{ width: 92 }}>activity</span><span style={{ width: 20, textAlign: "center" }}>=</span><span>resolved dot</span>
            </div>
            {[["agent", "running", "running"], ["agent", "idle", "agent"], ["agent", "waiting", "waiting"], ["user", "running", "running"], ["user", "idle", "idle"], ["agent", "error", "error"]].map(([owner, activity, res]) => (
              <div key={owner + activity} style={{ display: "flex", alignItems: "center", gap: 10, padding: "9px 0", borderBottom: "1px solid var(--tasty-separator)" }}>
                <span style={{ width: 92, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: owner === "agent" ? "var(--tasty-accent-agent)" : "var(--tasty-text-secondary)" }}>{owner}</span>
                <span style={{ width: 92, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-secondary)" }}>{activity}</span>
                <span style={{ width: 20, textAlign: "center", color: "var(--tasty-text-muted)" }}>→</span>
                <StatusDot status={res} pulse={res === "running" || res === "agent" || res === "waiting"} label={res} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["priority", "error › waiting › running › agent › idle"], ["rule", "live activity beats ownership"], ["dots shown", "always exactly 1"], ["helper", <span className="ic">resolveStatus(owner, activity)</span>]]}
            tokens={[{ tok: "--tasty-accent-success", use: "running (wins)", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-accent-agent", use: "agent (idle only)", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-accent-danger", use: "error (top)", color: "var(--tasty-accent-danger-raised)" }]} />
          <Dont><b>Don't</b> stack an ownership dot next to an activity dot. One surface = one dot. If you think you need two, you're encoding two facts that the priority order already resolves into one.</Dont>
        </Spec>

        <Spec id="spinner" title="Spinner"
          when={<>Indeterminate progress for <b>short background work</b> — a port scan, plugin install, surface attach, remote shell detection. A thin rotating arc on a faint track, inheriting the current text color so it sits inline in muted contexts or recolors on an accent button. <b>Not</b> for determinate progress (use a bar) or long waits. Honors <code>prefers-reduced-motion</code> — falls back to three static dots.</>}>
          <Stage style={{ gap: 28 }}>
            <Cluster label="sizes — 12 · 16 · 20 · 24">
              <Spinner size={12} /><Spinner size={16} /><Spinner size={20} /><Spinner size={24} />
            </Cluster>
            <Cluster label="inline with text">
              <span style={{ display: "inline-flex", alignItems: "center", gap: 8, fontSize: 13, color: "var(--tasty-text-muted)" }}>
                <Spinner size={14} /> Collecting…
              </span>
            </Cluster>
            <Cluster label="beside a button · detecting row">
              <Button variant="secondary" disabled>Install</Button>
              <span style={{ display: "inline-flex", alignItems: "center", gap: 6, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}><Spinner size={14} />Installing…</span>
              <span style={{ display: "inline-flex", alignItems: "center", gap: 6, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>
                <Spinner size={12} /> detecting shell…
              </span>
            </Cluster>
          </Stage>
          <Meta
            specs={[["sizes", "12 / 16 / 20 / 24px"], ["stroke", "2px arc + faint track"], ["track", "arc ink at 22%, ornament, no grade"], ["spin", "0.9s linear"], ["color", <span className="tok">currentColor</span>], ["reduced motion", "→ 3 static dots"]]}
            tokens={[{ tok: "--tasty-text-muted", use: "default color", color: "var(--tasty-text-muted)" }, { tok: "--tasty-spinner-size", use: "diameter" }, { tok: "--tasty-size-16", use: "16px default" }]} />
          <Note>It inherits <span className="ic">color</span> — drop it on a filled button and it turns <span className="ic">--tasty-text-on-accent</span> automatically. Used by the Port Scanner scan state and the Remote tool’s shell-detection row.</Note>
        </Spec>

        <Spec id="toast" title="Toast"
          when={<>Transient notification card with an accent rail by intent — <code>Copied</code>, <code>Path copied</code>, <code>Force detach</code>. Terse, second person. <b>hint (2026-10-06)</b> is the shortcut of the action that raised the notice, shown only when the action came from a <b>menu or the mouse</b> (pressing the key already taught it) and only if the binding is set — an unbound action shows no hint. It sits at the right end on the <b>first line</b>; the body wraps before the hint ever shrinks. The <b>agent</b> variant is catalog-only: the host never toasts agent-originated results (they are logged), so no host notice uses it.</>}>
          <Stage variant="column" style={{ gap: 10, alignItems: "stretch", maxWidth: 380 }}>
            <Toast variant="success" hint={<Kbd keys="⌘⇧C" />}>Path copied</Toast>
            <Toast variant="success">Path copied</Toast>
            <Toast variant="success" hint={<Kbd keys="⌘C" />}>Copied 3 lines from the selection to the clipboard</Toast>
            <Toast variant="info">Copied (OSC 52)</Toast>
            <Toast variant="warning">Held by another client (readonly)</Toast>
            <Toast variant="danger">Force detach — connection dropped</Toast>
          </Stage>
          <Meta
            specs={[["rail", "3px accent left edge"], ["radius", <span className="tok">--tasty-radius</span>], ["fill", <span className="tok">--tasty-surface-raised</span>], ["boxes", "rail · body (flex 1) · hint (flex none) · gap space-sm · pad space-sm / space-md"], ["hint", "Kbd of the action's binding · menu / mouse origin only · none when unbound"], ["hint position", "right end, first line · never truncated — body wraps first"], ["platform", "Kbd rule: ⌘ ⇧ ⌥ on macOS, Ctrl+Shift+ on Windows / Linux"], ["agent", "catalog only — host does not emit"], ["icon", "catalog only — host card has no icon"]]}
            tokens={[{ tok: "--tasty-accent-success", use: "ok rail", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-toast-hint-font-size", use: "hint · micro mono" }, { tok: "--tasty-surface-raised", use: "card", color: "var(--tasty-surface-raised)" }]} />
        </Spec>

        <Spec id="toast-stack" title="Toast stack"
          when={<>When several notices fire close together they <b>stack</b> rather than replace — they grow <b>upward from the anchor corner</b>, so the newest card sits at the bottom and older ones are pushed up, each keeping its own intent rail. A scope holds at most <b>5</b> at once; a sixth arriving drops the oldest (topmost) card immediately. Cards that would run past the scope's top edge are simply not drawn — there is no overflow row. Every floating card is fully opaque regardless of age; alpha is only for enter / exit. Same card as a single Toast; only the layout (vertical gap, order) is new. <b>Width (2026-09-29):</b> each card keeps its <b>own content width</b>, capped at <span className="tok">--tasty-toast-max-width</span>; the stack aligns <b>right edges</b> to the anchor, so a short notice is narrower than a long one. There is no shared stack width, so nothing re-flows when a card enters or leaves.</>}>
          <Stage variant="solo" style={{ padding: 24, background: "var(--tasty-bg-app)", display: "flex", justifyContent: "flex-end", alignItems: "flex-end", minHeight: 300 }}>
            <div style={{ width: "var(--tasty-toast-max-width)", display: "flex", flexDirection: "column", justifyContent: "flex-end", alignItems: "flex-end", gap: "var(--tasty-space-sm)" }}>
              <Toast variant="info">Two notices while importing the bundle</Toast>
              <Toast variant="warning">Held by another client (readonly)</Toast>
              <Toast variant="info">Settings applied</Toast>
              <Toast variant="success" hint={<Kbd keys="⌘C" />}>Path copied to clipboard</Toast>
              <Toast variant="danger">Force detach — connection dropped</Toast>
            </div>
          </Stage>
          <Meta
            specs={[["anchor", "one corner (bottom-right)"], ["order", "newest bottom"], ["gap", <>8px <span className="tok">--tasty-space-sm</span></>], ["cap", "5 per scope → oldest dropped"], ["width", <>content width per card, cap <span className="tok">--tasty-toast-max-width</span> (320) · right edges align</>], ["window-scope offset (b2 · 2026-10-07)", <>main window: <span className="tok">--tasty-toast-stack-offset-bottom</span> (→ size-36 = status bar 24 + 12), OFF-SCALE like the status bar and the card dims · Settings window: <span className="tok">--tasty-toast-stack-offset-bottom-settings</span> (→ size-64 = footer 52 + 12), ON-SCALE like the footer — the stack sits above Cancel / Save</>], ["scale", "every structural toast dim (scope margin 12, padding 12 × 8, gap 8, main offset) is OFF-SCALE; only the Settings offset follows the zoom"], ["shared corner (2026-10-07)", "workspace- and pane-scope stacks that land on the same corner merge into ONE column, newest bottom, cap 5 across both"], ["link menu Copy hint (b2)", "own action copy_link ('Copy link'), unbound by default → no hint until the user binds it; never borrow the copy binding"], ["over a native WebView (b2)", "while a toast card's rect intersects a WebView, only THAT WebView is hidden for the card's life (its tile shows plain); others stay; keyboard focus is not reclaimed (a toast never takes focus). A WebView that is RECEIVING KEYS is not hidden, so the card stays under it — accepted (2026-10-07): typing must not be lost, and a toast is transient"]]}
            tokens={[{ tok: "--tasty-space-sm", use: "stack gap" }, { tok: "--tasty-surface-raised", use: "each card", color: "var(--tasty-surface-raised)" }]} />
          <Dont><b>Don’t</b> let the stack grow unbounded, and don’t fold the tail into a “+N more” row either. Hold the cap by dropping the oldest card — a wall of toasts buries the newest signal, and an overflow counter is one more thing to read instead of the notice itself.</Dont>
        <Spec id="the-dot-family-8-generic-6-in-dense-chrome-and-the-attached-ring" title="The dot family — 8 generic, 6 in dense chrome, and the attached ring"
          when={<>Dots come in <b>one role</b> and <b>two footprints</b>. The generic status dot is <b>8</b> (badges, tags, list rows). Inside <b>24px-tall chrome</b> — the pane tab strip, the workspace status bar, the collapsed sidebar rail — it is <b>6</b> (<span className="tok">--tasty-status-dot-size-compact</span>): 8 crowds a 24px row the moment a ring or a label sits beside it. That settles three drifting numbers at once — the tab busy dot keeps its 6 (it is <b>not</b> an alias of the generic 8 any more), the status bar's <b>7 becomes 6</b>, and the rail's 6 is now named rather than hard-coded. The <b>active tab marker</b> stays <b>4</b>: a different role (position, not state) and untouched. The <b>attached ring</b> adopts the token pair <b>2 / 2</b> — offset measured from the <b>dot's outer edge to the ring's inner edge</b> — replacing the 1.5 / 1.5 the product drew; in 24px chrome the whole mark is 6 + 2×(2+2) = <b>14</b> (11.9 / 14 / 16.8). <b>Corrected 2026-09-28:</b> the one place a ring actually ships is the expanded-sidebar <b>workspace row</b>, a list row on the generic 8 — its mark is 8 + 2×(2+2) = <b>16</b> (13.6 / 16 / 19.2) and it sits in a reserved 16px slot (next spec). The 14 figure applies to no shipping surface today.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 18, alignItems: "flex-start" }}>
            <div style={{ display: "flex", gap: 28, alignItems: "flex-end", flexWrap: "wrap" }}>
              {[["generic · 8", "var(--tasty-status-dot-size)", null], ["dense chrome · 6", "var(--tasty-status-dot-size-compact)", null], ["attached · 6 + ring 2/2", "var(--tasty-status-dot-size-compact)", true], ["tab marker · 4 (other role)", "4px", null]].map(([label, size, ring]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
                  <div style={{ height: 24, display: "flex", alignItems: "center", justifyContent: "center", width: 40, background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius-sm)" }}>
                    <span style={{ width: size, height: size, borderRadius: "var(--tasty-radius-pill)",
                      background: ring ? "var(--tasty-status-dot-success)" : "var(--tasty-status-dot-success)",
                      boxShadow: ring ? "0 0 0 var(--tasty-status-dot-attached-ring-offset) var(--tasty-bg-sidebar), 0 0 0 calc(var(--tasty-status-dot-attached-ring-offset) + var(--tasty-status-dot-attached-ring-width)) var(--tasty-status-dot-attached-ring)" : "none" }} />
                  </div>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{label}</span>
                </div>
              ))}
            </div>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
              {[["ui_scale 0.85", 0.85], ["1", 1], ["1.2", 1.2]].map(([label, z]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{label} — bbox {(14 * z).toFixed(1)} in a 24 bar</span>
                  <div style={{ height: 24, width: 150, display: "flex", alignItems: "center", gap: 8, padding: "0 8px", background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius-sm)" }}>
                    <span style={{ width: 6 * z, height: 6 * z, flex: "none", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-status-dot-success)",
                      boxShadow: `0 0 0 ${2 * z}px var(--tasty-bg-sidebar), 0 0 0 ${4 * z}px var(--tasty-status-dot-attached-ring)` }} />
                    <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>attached</span>
                  </div>
                </div>
              ))}
            </div>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap" }}>
              {[["toast gap 6 — the product today", 6], ["toast gap 8 — settled", 8]].map(([label, g]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: g === 8 ? "var(--tasty-accent-success-raised)" : "var(--tasty-text-muted)" }}>{label}</span>
                  <div style={{ display: "flex", flexDirection: "column", gap: g, width: 260 }}>
                    <Toast variant="success">Keybindings exported.</Toast>
                    <Toast variant="info">Two notices while importing the bundle — open Import / Export to read them.</Toast>
                  </div>
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["generic dot", <>8 — <span className="tok">--tasty-status-dot-size</span> (unchanged)</>], ["dense chrome", <>6 — <span className="tok">--tasty-status-dot-size-compact</span> (new)</>], ["tab busy dot", "6 — keeps its value, now its own role"], ["status bar dot", "7 → 6 (visible change)"], ["sidebar rail dot", "6 — unchanged, now tokenised"], ["active tab marker", "4 — different role, untouched"], ["attached ring", "width 2 / offset 2 (1.5 → 2, visible change)"], ["attached bbox", "compact 6 → 14 · generic 8 → 16 (workspace row, the only ring in product)"], ["offset measured", "dot outer edge → ring inner edge"], ["toast gap", "6 → 8 (visible change, 4px grid)"], ["badge / tag dots", "unchanged at 8"]]}
            tokens={[{ tok: "--tasty-status-dot-size-compact", use: "tab · status bar · rail" }, { tok: "--tasty-tab-dot-size", use: "→ compact" }, { tok: "--tasty-statusbar-dot-size", use: "→ compact" }, { tok: "--tasty-status-dot-attached-ring-width", use: "2" }, { tok: "--tasty-status-dot-attached-ring-offset", use: "2" }, { tok: "--tasty-toast-gap", use: "8, unchanged token" }]} />
          <Dont><b>Don't</b> push the whole family to one number. A dot in a 24px strip and a dot on a 36px list row are the same <i>meaning</i> at two <i>densities</i>; collapsing them either crowds the strip or shrinks every badge.</Dont>
          <Note><b>Unchanged by this decision:</b> every badge, tag and status consumer of the generic 8; the 4px active marker; the dot colours and their meanings (idle / busy / attached / needs-input / completion).</Note>
        </Spec>
        <Spec id="attached-ring-in-the-workspace-row-a-reserved-16px-slot" title="Attached ring in the workspace row — a reserved 16px slot"
          when={<>The workspace row keeps the generic <b>8</b> dot, so the attached mark is <b>16</b>. The row reserves a <b>16px dot slot</b> (<span className="tok">--tasty-workspace-dot-slot</span>) on <b>every</b> row, attached or not, and centres the dot in it. Row left inset <b>8</b> (<span className="tok">--tasty-workspace-row-padding-x</span>), slot → body <b>4</b> (<span className="tok">--tasty-workspace-dot-gap</span>). Body x = 8 + 16 + 4 = <b>28</b> for title, remote pill and subtitle alike. Clearances from the ring's outer edge at scale 1: card edge <b>8</b>, active accent bar (x 0–2) <b>6</b>, label <b>4</b>. All three are tokens and scale with ui_scale. <b>Rounding (2026-09-29):</b> the slot is <b>derived after rounding</b>, never rounded on its own: slot = round(dot) + 2 × (round(ring-width) + round(ring-offset)). At 0.85 that is 7 + 2 × (2 + 2) = <b>15</b> (not round(13.6) = 14), so the ring always fits its slot; at 1.2 it is 10 + 8 = <b>18</b>. The slot is square and its height is the title's own line box, so it never adds row height. The active bar is a hairline (<span className="tok">--tasty-workspace-row-active-bar-width</span>, 2, zoom-exempt). Compact 6 was rejected: this is a list row, and the 8 = list rows rule stays exception-free.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-lg)", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>
                <span style={{ width: "100%", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · ui_scale 1</span>
                {[["active", { active: true }], ["inactive", {}], ["hover", { hover: true }]].map(([st, p]) => (
                  <div key={st} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                    <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{st}</span>
                    <WsColG>
                      <WsRowG {...p} attached name="second" />
                      <WsRowG {...p} name="api-server" />
                      <WsRowG {...p} attached name="staging" pill sub="ssh deploy@10.0.4.12" />
                    </WsColG>
                  </div>
                ))}
              </div>
            ))}
            <div style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
              {[["ui_scale 0.85 — bbox 13.6", 0.85], ["ui_scale 1.2 — bbox 19.2", 1.2]].map(([cap, z]) => (
                <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                  <div style={{ zoom: z }}><WsColG><WsRowG active attached name="second" /><WsRowG attached name="staging" pill sub="ssh deploy@10.0.4.12" /></WsColG></div>
                </div>
              ))}
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-accent-danger-raised)" }}>rejected — product today (inset 4 · slot 8 · gap 4)</span>
                <div style={{ zoom: 2 }}><WsColG><WsRowG geom="product" active attached name="second" /></WsColG></div>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-accent-success-raised)" }}>settled — 2× zoom (inset 8 · slot 16 · gap 4)</span>
                <div style={{ zoom: 2 }}><WsColG><WsRowG active attached name="second" /></WsColG></div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["dot", <>8 — <span className="tok">--tasty-status-dot-size</span> (unchanged)</>], ["attached bbox", "8 + 2×(2+2) = 16 → 13.6 / 16 / 19.2"], ["row inset", "product 4 → 8"], ["dot slot", "product 8 → 16, reserved on every row"], ["slot → body", "4 (unchanged)"], ["body x", "product 12 → 28 (title · pill · subtitle)"], ["clearance", "card 8 · accent bar 6 · label 4"], ["ui_scale 0.85", "inset 7 · slot 15 (derived) · gap 3 → ring 7..22, label x 25 · clear card 7 · bar 5 · label 3"], ["ui_scale 1.2", "inset 10 · slot 18 · gap 5 → label x 33 · row height unchanged"], ["collapsed rail", "unchanged — avatar square border, no ring"]]}
            tokens={[{ tok: "--tasty-workspace-row-padding-x", use: "→ space-sm 8" }, { tok: "--tasty-workspace-dot-slot", use: "→ size-16" }, { tok: "--tasty-workspace-dot-gap", use: "→ space-xs 4" }, { tok: "--tasty-workspace-row-active-bar-width", use: "→ selection-edge-width 2 (hairline)" }, { tok: "--tasty-status-dot-attached-ring", use: "lavender", color: "var(--tasty-status-dot-attached-ring)" }]} />
          <Dont><b>Don't</b> size the slot to the dot and let the outline overflow into padding and gap. The kit's 8 / 8 held the label clear, but the ring still reached the active accent bar. Reserve the whole mark.</Dont>
        </Spec>
        </Spec>
      </Section>

      <Section id="centerstate" title="CenterState — empty · loading · error">
        <Spec id="one-centred-block-for-every-empty-list" title="One centred block for every empty list"
          when={<>The file picker, the remote-attach popup and <b>Settings › Misc › Scripts</b> all show the same block where their list would be: glyph or spinner, a title line, and an optional sub line. It is <b>one part</b> with three variants. Glyph = <b>24</b> (<span className="tok">--tasty-icon-size-lg</span>, new icon tier) for both the old centre glyph 22 and the scripts glyph 26. <b>No fixed height</b>: the block centres in the region it replaces, so the product's 100 and the specimen's 120 both go. The sub line slot is <b>always reserved</b>, one caption line even when empty, so loading → empty → error never moves the glyph. Everything is a token, so the block scales with ui_scale (0.85 / 1 / 1.2).</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-lg)", alignItems: "flex-start" }}>
            {[["file picker · remote attach", "fp"], ["Settings › Misc › Scripts", "scripts"]].map(([host, k]) => (
              <div key={k} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{host}</span>
                <div style={{ display: "flex", gap: "var(--tasty-space-md)", flexWrap: "wrap" }}>
                  {[["loading"], ["empty"], ["error"], ["error", true]].map(([v, act]) => (
                    <div key={v + (act ? "-a" : "")} style={{ width: "var(--tasty-size-288)", height: k === "fp" ? "var(--tasty-size-220)" : "var(--tasty-size-160)", display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                      <div style={{ height: "var(--tasty-control-height)", flex: "none", display: "flex", alignItems: "center", padding: "0 var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>{k === "fp" ? "~/work/tasty/assets" : "Scripts · " + v + (act ? " + action" : "")}</div>
                      <CenterStateG variant={v} host={k} action={act} />
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["variants", "loading · empty · error"], ["glyph / spinner", "24 — icon-size-lg (was 22 · 26)"], ["glyph → title", "8"], ["title → sub", "4 · sub slot always reserved"], ["title", "body 13 · text-secondary"], ["sub", "caption 11 · text-muted · wraps at 300"], ["height", "none — centres in the list region; unsized hosts (natural): block + 2 × 12, with action block + 2 × 48"], ["ui_scale", "scales (tokens) — 20.4 / 24 / 28.8 glyph"]]}
            tokens={[{ tok: "--tasty-center-state-glyph-size", use: "→ icon-size-lg 24" }, { tok: "--tasty-center-state-glyph-fg", use: "→ glyph-dim", color: "var(--tasty-center-state-glyph-fg)" }, { tok: "--tasty-center-state-error-fg", use: "→ accent-danger", color: "var(--tasty-center-state-error-fg)" }, { tok: "--tasty-center-state-gap", use: "8" }, { tok: "--tasty-center-state-line-gap", use: "4" }, { tok: "--tasty-center-state-max-width", use: "→ measure-sm 300" }]} />
          <Note><b>Clipboard centre glyph 28</b> is a content glyph (T6, 2026-09-17) and stays outside this part.</Note>
        </Spec>
        <Spec id="error-glyph-and-the-action-slot" title="Error glyph and the action slot"
          when={<>The part owns the <b>error glyph</b>: always <b>alertTriangle</b>, in every host (<code>CENTER_STATE_ERROR_GLYPH</code>). Hosts choose only the <b>empty</b> glyph (folderOpen, scriptFile, remote …). The optional <b>action</b> (Retry / Reconnect / New workspace) is a part slot: <b>Button secondary · sm</b>, <span className="tok">--tasty-center-state-action-gap</span> (12) below the reserved sub slot, horizontally centred. It is <b>outside the centring</b>: only glyph · title · sub are centred, so error with and without an action puts the glyph in the same place (see the last column above). The block is now the DS component <span className="ic">CenterState</span>; the file picker and remote attach frames render it.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-md)", flexWrap: "wrap" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                {[false, true].map((act) => (
                  <div key={String(act)} style={{ width: "var(--tasty-size-220)", height: "var(--tasty-size-220)", display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                    <CenterStateG variant="error" host="fp" action={act} />
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["error glyph", "alertTriangle — part-owned, every host"], ["empty glyph", "host-chosen"], ["action", "Button secondary · sm (24)"], ["sub → action", "12 · center-state-action-gap"], ["centring", "glyph · title · sub only — action hangs below"], ["short region", "action may reach the region's bottom padding; never pushes the glyph"]]}
            tokens={[{ tok: "--tasty-center-state-action-gap", use: "→ space-md 12" }, { tok: "--tasty-button-height-sm", use: "24" }, { tok: "--tasty-center-state-error-fg", use: "→ accent-danger", color: "var(--tasty-center-state-error-fg)" }]} />
          <Dont><b>Don't</b> put the button in the centred column. Loading → error + Retry would jump the glyph up by half the button, and with no animation that reads as a glitch.</Dont>
        </Spec>
        <Spec id="unsized-host-action-symmetric-natural-height" title="Unsized host + action — symmetric natural height"
          when={<>When the host gives <b>no height</b> (<code>natural</code>; the Settings › Misc › Scripts list is the one such host today) the part takes its own height. Without an action that is the block plus <span className="tok">--tasty-space-md</span> above and below. With an action the part reserves the <b>action band on both sides</b>: padding-block = space-md + <span className="tok">--tasty-center-state-action-gap</span> + <span className="tok">--tasty-button-height-sm</span> = <b>48</b>. The block stays centred, so the glyph sits where it does without the action; the action ends <b>space-md (12)</b> above the bottom edge and never overflows into the next widget. The cost is 36 of empty space above the glyph, accepted. The cards below have no fixed height; the part sets it.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-md)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", gap: "var(--tasty-space-md)", alignItems: "flex-start", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                {[["empty", false], ["error + action", true]].map(([capt, act]) => (
                  <div key={capt} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                    <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{label} · {capt}</span>
                    <div style={{ width: "var(--tasty-size-220)", display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                      <CenterState natural variant={act ? "error" : "empty"} glyph="scriptFile" title={CS_COPY.scripts[act ? "error" : "empty"][0]} sub={CS_COPY.scripts[act ? "error" : "empty"][1]}
                        action={act ? <Button variant="secondary" size="sm" leadingIcon={<CIcon name="refresh" size="var(--tasty-icon-size-sm)" />}>Retry</Button> : null} />
                      <div style={{ height: "var(--tasty-control-height)", flex: "none", display: "flex", alignItems: "center", padding: "0 var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", borderTop: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>next widget</div>
                    </div>
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["prop", "natural — host gives no height"], ["no action", "padding-block space-md (12)"], ["with action", "padding-block 12 + 12 + 24 = 48, both sides"], ["block", "stays centred — glyph position independent of the action"], ["below the action", "space-md (12) — no new token"], ["rejected", "top-pinned block (glyph moves) · current overflow (6px into the next widget)"]]}
            tokens={[{ tok: "--tasty-space-md", use: "12 outer pad" }, { tok: "--tasty-center-state-action-gap", use: "→ space-md 12" }, { tok: "--tasty-button-height-sm", use: "24" }]} />
        </Spec>
      </Section>

      {/* HELPHINT / TOOLTIP */}
      <Section id="helphint" title="HelpHint · Tooltip">
        <Spec id="helphint-2" title="HelpHint"
          when={<>An inline circular <code>?</code> after a label that explains it on <b>hover</b> — how we move a below-control description up beside its label so a settings row stays a single line. Muted at rest, brightens on hover / focus; anchors a <span className="ic">Tooltip</span>. Hover-only, no click; cursor is <code>help</code>.</>}>
          <Stage style={{ gap: 30, alignItems: "flex-start", paddingTop: 34 }}>
            <Cluster label="in a settings row — hover the (?)">
              <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
                <span style={{ width: 150, flex: "none", display: "inline-flex", alignItems: "center", gap: "var(--tasty-help-hint-gap)", fontSize: 13, color: "var(--tasty-text-secondary)" }}>Scrollback disk swap: <HelpHint label="Spill scrollback past the in-memory limit to a temp file instead of dropping the oldest lines." placement="bottom" /></span>
                <Switch defaultChecked />
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 16, marginTop: 8 }}>
                <span style={{ width: 150, flex: "none", display: "inline-flex", alignItems: "center", gap: "var(--tasty-help-hint-gap)", fontSize: 13, color: "var(--tasty-text-secondary)" }}>PTY polling (ms): <HelpHint label="How often the PTY read loop is polled. Lower = snappier output at higher CPU cost. Default 8ms." placement="bottom" /></span>
                <Input mono defaultValue="8" style={{ width: "var(--tasty-field-width-xs)" }} />
              </div>
            </Cluster>
            <Cluster label="rest / hover">
              <div style={{ display: "flex", alignItems: "center", gap: 22, color: "var(--tasty-text-secondary)", fontSize: 13 }}>
                <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>rest <HelpHint /></span>
                <span style={{ display: "inline-flex", alignItems: "center", gap: 6, color: "var(--tasty-text-secondary)" }}>hover <span style={{ color: "var(--tasty-help-hint-color-hover)", display: "inline-flex" }}><GIcon d={<><circle cx="12" cy="12" r="10" /><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" /><path d="M12 17h.01" /></>} size={14} /></span></span>
              </div>
            </Cluster>
          </Stage>
          <Meta
            specs={[
              ["glyph", <>help-circle · <span className="tok">--tasty-help-hint-size</span> 14</>],
              ["rest", <span className="tok">--tasty-help-hint-color</span>],
              ["hover", <span className="tok">--tasty-help-hint-color-hover</span>],
              ["gap", <>4px <span className="tok">--tasty-help-hint-gap</span></>],
              ["cursor", "help — no click"],
            ]}
            tokens={[
              { tok: "--tasty-help-hint-color", use: "rest glyph", color: "var(--tasty-help-hint-color)" },
              { tok: "--tasty-help-hint-color-hover", use: "hover glyph", color: "var(--tasty-help-hint-color-hover)" },
              { tok: "--tasty-help-hint-gap", use: "label → (?)" },
            ]} />
          <Note>Replaces the below-control caption on dense pages (<b>Settings › Terminal › Performance</b>). The description copy is unchanged — it just moves into the bubble. Don't stack a HelpHint <i>and</i> a caption line for the same control.</Note>
        </Spec>

        <Spec id="tooltip" title="Tooltip"
          when={<>The hover bubble itself — opaque raised card, 1px edge, popover lift, <b>no arrow</b> (calm, low-chrome). Wraps any anchor; shows on hover or keyboard focus after a short delay. Reusable beyond HelpHint. Shown here forced-open via <code>open</code>.</>}>
          <Stage style={{ gap: 56, alignItems: "center", justifyContent: "center", paddingTop: 46, paddingBottom: 46, minHeight: 150 }}>
            {["top", "bottom", "left", "right"].map((p) => (
              <Tooltip key={p} open placement={p} content={p === "left" || p === "right" ? "Terse hint." : "A short hint that wraps to two or three lines when the copy is longer."}>
                <Tag>{p}</Tag>
              </Tooltip>
            ))}
          </Stage>
          <Meta
            specs={[
              ["bg", <span className="tok">--tasty-tooltip-bg</span>],
              ["border", <>1px <span className="tok">--tasty-tooltip-border</span></>],
              ["radius", <span className="tok">--tasty-tooltip-radius</span>],
              ["max-width", <>240 <span className="tok">--tasty-tooltip-max-width</span></>],
              ["placement", "top / bottom / left / right"],
              ["over native content", "tab strip · pane head: top → bottom → inside the strip (1px border-width tolerance) · none clears → inside the strip — Layouts › Pane tab strip. No anchor cell (outside a strip): top clamped, 4 from the window edge"],
              ["delay", <>150ms <span className="tok">--tasty-tooltip-delay</span></>],
            ]}
            tokens={[
              { tok: "--tasty-tooltip-bg", use: "bubble fill", color: "var(--tasty-tooltip-bg)" },
              { tok: "--tasty-tooltip-border", use: "1px edge", color: "var(--tasty-tooltip-border)" },
              { tok: "--tasty-tooltip-fg", use: "copy", color: "var(--tasty-tooltip-fg)" },
              { tok: "--tasty-tooltip-shadow", use: "lift" },
            ]} />
          <Dont><b>Don't</b> put actions, links, or long paragraphs in a tooltip — it's a hover hint, not a popover. If it needs a click target, that's a MenuItem popup.</Dont>
        </Spec>
      </Section>

      {/* TEXT */}
      <Section id="text" title="Hint text">
        <Spec id="hint-text" title="Hint text"
          when={<>The system’s <b>secondary helper line</b> — a sub-label under a field, a one-line keyboard hint in a dialog, the explanatory caption beneath a setting. Always <b>--tasty-text-muted</b>, 11–12px, sentence case, terse. It explains <i>without</i> competing with the primary label. Inline <code>Kbd</code> and <span className="ic">mono</span> spans are allowed; full sentences are not the goal.</>}>
          <Stage variant="column" style={{ gap: 16, alignItems: "stretch", maxWidth: 420 }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>Remote tasty path</span>
              <Input mono placeholder="/usr/local/bin/tasty" style={{ width: "100%" }} />
              <span style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5 }}>Leave empty to auto-detect on first connect.</span>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>Reduced motion</span>
              <span style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5 }}>Disables the terminal cursor blink and spinner animation.</span>
            </div>
            <span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>Press <Kbd keys="↵" /> to confirm, <Kbd keys="Esc" /> to cancel.</span>
          </Stage>
          <Meta
            specs={[["size", "11–12px"], ["color", <span className="tok">--tasty-text-muted</span>], ["case", "sentence case"], ["line-height", "1.5"], ["placement", "below the thing it explains"]]}
            tokens={[{ tok: "--tasty-text-muted", use: "hint color", color: "var(--tasty-text-muted)" }, { tok: "--tasty-font-size-caption", use: "11px" }, { tok: "--tasty-font-mono", use: "inline code" }]} />
          <Note>This is the same muted treatment used in the palette footer (<span className="ic">↑↓ navigate</span>), the Rename dialog hint, and every form sub-label — one consistent voice for “the quiet line.”</Note>
        </Spec>
        <Spec id="cli-runs-inside-ui-copy" title="CLI runs inside UI copy"
          when={<>The proportional UI face joins two hyphens into one dash, so <code>--webhook-port</code> reads as an en dash. Any <b>CLI command, option or argument</b> inside a UI sentence (error screen, banner, settings caption, result card) is drawn as a <b>code run</b>: mono at the <b>sentence's own size</b>, a surface-raised fill, 4px side padding, no vertical padding (the line height does not change), radius-sm, text-primary ink. The run never breaks inside; the sentence wraps around it. The copy keeps the literal option, so it can be typed back. Terminal content is not affected.</>}>
          <Stage variant="column" style={{ gap: 16, alignItems: "stretch", maxWidth: 420 }}>
            {[["13 body", 13, "var(--tasty-text-secondary)"], ["11 caption, muted", 11, "var(--tasty-text-muted)"]].map(([lab, fs, fg]) => (
              <div key={lab} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span style={{ fontSize: "var(--tasty-font-size-micro)", fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-muted)" }}>{lab}</span>
                <span style={{ fontSize: fs, color: fg, lineHeight: 1.5 }}>Port 7420 is in use. Start Tasty with <code style={{ fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", background: "var(--tasty-ui-code-bg)", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)", whiteSpace: "nowrap" }}>--webhook-port 7421</code> or close the other app.</span>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["font", "ui-code-font → font-mono · size = the sentence (1em)"], ["fill", "ui-code-bg → surface-raised"], ["ink", "ui-code-fg → text-primary"], ["padding", "ui-code-padding-x 4 · no vertical"], ["radius", "ui-code-radius → radius-sm"], ["wrap", "never inside the run"], ["scope", "UI copy only · not terminal content · i18n strings keep the literal option"]]}
            tokens={[{ tok: "--tasty-ui-code-font", use: "mono" }, { tok: "--tasty-ui-code-bg", use: "fill", color: "var(--tasty-ui-code-bg)" }, { tok: "--tasty-ui-code-fg", use: "ink", color: "var(--tasty-ui-code-fg)" }, { tok: "--tasty-ui-code-padding-x", use: "→ space-xs" }, { tok: "--tasty-ui-code-radius", use: "→ radius-sm" }]} />
        </Spec>
        <Spec id="code-runs-on-raised-containers-info-modal-chips-line-start-centr" title="Code runs — on raised containers · info modal chips · line start · centred hints"
          when={<><b>On a raised container</b> (toast card, tooltip, any surface-raised box) the run's fill would equal the card, so it switches to <span className="tok">--tasty-ui-code-bg-on-raised</span> (bg-panel — a recessed well; ink on it Mocha 11.34 · Latte 7.06). Rule: the fill is always one step <i>off</i> the container — surface-raised on panels and app bg, bg-panel on raised. <b>Info modal command chips</b> become ordinary code runs: sentence size, 4px padding, radius-sm (the caption-size chip without padding goes). <b>A run that starts a wrapped line</b> keeps its text on the column and lets the fill extend 4px outward — acceptable, because every container that holds copy has at least space-sm inset. <b>Centred hints</b> centre every line, including the line the run moved to.</>}>
          <Stage variant="column" style={{ gap: 16, alignItems: "stretch", maxWidth: 420 }}>
            <div style={{ padding: "var(--tasty-space-sm) var(--tasty-space-md)", background: "var(--tasty-toast-bg, var(--tasty-surface-raised))", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", fontSize: 13, color: "var(--tasty-text-primary)", lineHeight: 1.5 }}>
              Copied. Run <code style={{ ...{ fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)", whiteSpace: "nowrap" }, background: "var(--tasty-ui-code-bg-on-raised)" }}>tasty agent task-list</code> to see it.
            </div>
            <div style={{ fontSize: 13, color: "var(--tasty-text-secondary)", lineHeight: 1.5 }}>Reset the permission with <code style={{ ...{ fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)", whiteSpace: "nowrap" }, background: "var(--tasty-ui-code-bg)" }}>tccutil reset</code>, then sign the build with <code style={{ ...{ fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)", whiteSpace: "nowrap" }, background: "var(--tasty-ui-code-bg)" }}>./scripts/macos-codesign-identity.sh --create</code>.</div>
            <div style={{ alignSelf: "center", maxWidth: 260, textAlign: "center", fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5 }}>Create one with: <code style={{ ...{ fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)", whiteSpace: "nowrap" }, background: "var(--tasty-ui-code-bg)" }}>tasty agent task-create --workspace 1 ...</code></div>
          </Stage>
          <Meta
            specs={[["on raised", "ui-code-bg-on-raised → bg-panel (toast · tooltip · raised cards)"], ["elsewhere", "ui-code-bg → surface-raised (unchanged)"], ["info modal", "command chips = code runs (sentence size · padding-x 4 · radius-sm)"], ["line start", "text on the column · fill may extend 4 outside it"], ["centred hint", "every line centred (text-align center)"]]}
            tokens={[{ tok: "--tasty-ui-code-bg-on-raised", use: "→ bg-panel", color: "var(--tasty-ui-code-bg-on-raised)" }, { tok: "--tasty-ui-code-bg", use: "→ surface-raised", color: "var(--tasty-ui-code-bg)" }]} />
        </Spec>
      </Section>

      {/* DATA */}
      <Section id="data" title="Table">
        <Spec id="table" title="Table"
          when={<>Column-config data table for <b>system data</b> — listening ports, processes, sessions, keybindings. Data cells default to <b>--tasty-font-mono</b> so ports/PIDs/addresses align by glyph. Headers are <b>sortable</b> and <b>sticky</b> (wrap in a scroll container); rows hover and select. Right-align numeric columns.</>}>
          <Stage variant="solo" style={{ padding: 0, overflow: "hidden" }}>
            <div style={{ width: "100%", maxHeight: 280, overflow: "auto" }}>
              <PortsTableDemo />
            </div>
          </Stage>
          <Meta
            specs={[["row height", <>28px <span className="tok">--tasty-control-height</span> (dense 22)</>], ["header", <>sticky, <span className="tok">--tasty-bg-sidebar</span>, mono caption</>], ["cell font", <>mono columns → <span className="tok">--tasty-font-mono</span></>], ["selected", <span className="tok">--tasty-surface-active</span>], ["hairline", <span className="tok">--tasty-separator</span>]]}
            tokens={[
              { tok: "--tasty-bg-sidebar", use: "header row", color: "var(--tasty-bg-sidebar)" },
              { tok: "--tasty-surface-active", use: "selected row", color: "var(--tasty-surface-active)" },
              { tok: "--tasty-overlay-hover", use: "row hover" },
              { tok: "--tasty-accent-primary", use: "sort arrow", color: "var(--tasty-accent-primary-raised)" },
              { tok: "--tasty-font-mono", use: "port/addr/pid cells" },
            ]} />
          <Note>Columns can be marked <span className="ic">tight</span> — zero horizontal padding + centered — for narrow icon/control columns (the Listening-ports star toggle sits in a tight 28px leading column).</Note>
          <Note>This is what the <b>Tools › Listening ports</b> popup is built from. Use <span className="ic">render(value, row)</span> to compose cells — e.g. a <span className="ic">StatusDot</span> for the state column or a <span className="ic">Tag</span> for the PID.</Note>
        </Spec>
      </Section>

      {/* LISTCTRL */}
      <Section id="listctrl" title="ListCtrl">
        <Spec id="listctrl-navigation-list" title="ListCtrl — navigation list"
          when={<>A full-width, row-selectable <b>navigation</b> list — "pick one to <b>drill into</b>", not a data grid (that's <span className="ic">Table</span>). Each row is a <b>primary label</b> with an optional <b>description</b>, leading icon, a <b>trailing slot</b> (a Tag/Badge — e.g. <span className="ic">Active</span>), and a <b>drill-in chevron</b>. Rows share the list-idiom states: hover → <b>--tasty-overlay-hover</b>, selected → <b>--tasty-surface-active</b> + a 2px accent left bar, divided by the separator hairline. Pair it with the <b>Drill-down</b> layout (see Layouts) for the list → detail swap — this is the new <b>Settings › Keybindings › Preset</b> list.</>}>
          <Stage variant="solo" style={{ padding: 0, overflow: "hidden" }}>
            <div style={{ width: "100%" }}><ListCtrlDemo /></div>
          </Stage>
          <Meta
            specs={[["row", <>label · [description] · [trailing] · chevron</>], ["min height", <>36px <span className="tok">--tasty-listctrl-row-min-height</span></>], ["hover", <span className="tok">--tasty-overlay-hover</span>], ["selected", <><span className="tok">--tasty-surface-active</span> + 2px accent bar</>], ["chevron", "trailing drill-in affordance (default on)"], ["divider", <><span className="tok">--tasty-separator</span> hairline between rows</>], ["disabled", <><span className="tok">--tasty-state-disabled-fg</span> ink · no chevron · non-selectable · trailing Tag/Badge → disabled variant</>]]}
            tokens={[
              { tok: "--tasty-listctrl-row-bg-hover", use: "row hover", color: "var(--tasty-overlay-hover)" },
              { tok: "--tasty-listctrl-row-bg-selected", use: "selected row", color: "var(--tasty-surface-active)" },
              { tok: "--tasty-listctrl-selected-bar", use: "accent left bar", color: "var(--tasty-accent-primary-raised)" },
              { tok: "--tasty-listctrl-label-fg", use: "label", color: "var(--tasty-text-secondary)" },
              { tok: "--tasty-listctrl-desc-fg", use: "description", color: "var(--tasty-text-muted)" },
              { tok: "--tasty-listctrl-chevron-fg", use: "drill-in chevron", color: "var(--tasty-text-muted)" },
            ]} />
          <Do><b>Do</b> reach for <span className="ic">ListCtrl</span> when a row leads somewhere (drill-down, apply). <b>Don't</b> use it for tabular data with multiple comparable columns — that's <span className="ic">Table</span>. The <b>trailing slot</b> is for one status marker (Active / a count), not a second data column.</Do>
        </Spec>
        <Spec id="disabled-row-with-a-trailing-marker-the-ink-rule" title="Disabled row with a trailing marker — the ink rule"
          when={<>A disabled row follows the same rule as a disabled control: <b>ink, never opacity</b>. Icon, label, description take <span className="tok">--tasty-state-disabled-fg</span>; the chevron goes; no hover, not selectable. The <b>trailing Tag / Badge drops to its disabled variant</b>: the neutral box (<span className="tok">--tasty-tag-disabled-bg</span> / <span className="tok">-border</span>, <span className="tok">--tasty-badge-disabled-bg</span>) with the same ink. Accent fills and tint edges drop out, so the disabled ink never sits on an accent. The marker is not hidden: what it says (Active, a count) is still true of the row. Tag and Badge also take a <code>disabled</code> prop for use outside ListCtrl.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-md)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ width: "var(--tasty-size-320)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
                <ListCtrl selectedId="default" items={[
                  { id: "default", label: "Default", description: "enabled · success Tag", trailing: <Tag variant="success" dot>Active</Tag> },
                  { id: "d-success", label: "Readline", description: "disabled · success Tag", trailing: <Tag variant="success" dot>Active</Tag>, disabled: true },
                  { id: "d-accent", label: "Custom", description: "disabled · accent Tag", trailing: <Tag variant="accent">edited</Tag>, disabled: true },
                  { id: "d-badge", label: "Imported", description: "disabled · Badge", trailing: <Badge variant="primary">3</Badge>, disabled: true },
                ]} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["row ink", <span className="tok">--tasty-state-disabled-fg</span>], ["chevron", "hidden"], ["trailing", "kept · disabled variant (neutral box + ink)"], ["accent Tag", "fill drops out"], ["tint edge", <>→ <span className="tok">--tasty-state-disabled-border</span></>], ["opacity", "none"]]}
            tokens={[{ tok: "--tasty-tag-disabled-bg", use: "→ state-disabled-fill", color: "var(--tasty-tag-disabled-bg)" }, { tok: "--tasty-tag-disabled-border", use: "→ state-disabled-border", color: "var(--tasty-tag-disabled-border)" }, { tok: "--tasty-tag-disabled-fg", use: "→ state-disabled-fg", color: "var(--tasty-tag-disabled-fg)" }, { tok: "--tasty-badge-disabled-bg", use: "→ state-disabled-fill", color: "var(--tasty-badge-disabled-bg)" }]} />
        </Spec>
      </Section>

      <Section id="scrollbar" title="Scrollbar">
        {GalleryScrollbar && <GalleryScrollbar />}
      </Section>
    </>
  );
}

function ListCtrlDemo() {
  const [sel, setSel] = React.useState("default");
  const items = [
    { id: "default", label: "Default", description: "Tasty stock bindings", trailing: <Tag variant="success" dot>Active</Tag> },
    { id: "mac", label: "Mac", description: "⌘-based, native-app muscle memory" },
    { id: "emacs", label: "Emacs", description: "C-x / C-c prefix chords" },
    { id: "vim", label: "Vim", description: "modal, hjkl pane motions" },
    { id: "readline", label: "Readline", description: "GNU readline defaults", disabled: true },
  ];
  return <ListCtrl items={items} selectedId={sel} onSelect={(id) => setSel(id)} />;
}

function PortsTableDemo() {
  const [sort, setSort] = React.useState({ key: "port", dir: "asc" });
  const [sel, setSel] = React.useState(8080);
  const DATA = [
    { port: 22, proto: "tcp", addr: "0.0.0.0", proc: "sshd", pid: 712, state: "LISTEN" },
    { port: 3000, proto: "tcp", addr: "127.0.0.1", proc: "node", pid: 48213, state: "LISTEN" },
    { port: 5432, proto: "tcp", addr: "127.0.0.1", proc: "postgres", pid: 1192, state: "LISTEN" },
    { port: 8080, proto: "tcp", addr: "0.0.0.0", proc: "tasty-agent", pid: 50321, state: "LISTEN" },
    { port: 9229, proto: "tcp", addr: "127.0.0.1", proc: "node", pid: 48213, state: "CLOSE_WAIT" },
  ];
  const onSort = (key) => setSort((s) => ({ key, dir: s.key === key && s.dir === "asc" ? "desc" : "asc" }));
  const rows = [...DATA].sort((a, b) => {
    const av = a[sort.key], bv = b[sort.key];
    const c = typeof av === "number" ? av - bv : String(av).localeCompare(String(bv));
    return sort.dir === "asc" ? c : -c;
  });
  const columns = [
    { key: "port", header: "Port", align: "right", mono: true, sortable: true, width: "84px" },
    { key: "proto", header: "Proto", mono: true, width: "76px" },
    { key: "addr", header: "Address", mono: true, sortable: true },
    { key: "proc", header: "Process", strong: true, sortable: true,
      render: (v, row) => (<span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>{v}<Tag>{row.pid}</Tag></span>) },
    { key: "state", header: "State", width: "140px",
      render: (v) => <StatusDot status={v === "LISTEN" ? "running" : "waiting"} pulse={v === "LISTEN"} label={v} /> },
  ];
  return <Table columns={columns} rows={rows} rowKey="port" sort={sort} onSort={onSort}
    selectedKey={sel} onRowClick={(row) => setSel((k) => (k === row.port ? null : row.port))} />;
}

function HoverBadge({ label = "live" }) {
  return (
    <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-accent-primary-raised)",
      border: "1px solid var(--tasty-accent-primary-raised)", borderRadius: "var(--tasty-radius-sm)", padding: "1px 6px", fontWeight: 400 }}>
      {label}
    </span>
  );
}

export const page = {
  active: "components",
  nav: NAV,
  head: {
    title: "Components",
    intro: "The reusable primitives, live. Hover and focus respond for real — that's how you read the interaction states. Each specimen states when to reach for it, its fixed dimensions, and the exact tokens it's built from.",
    
  },
  content: <Components />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}

// The tokens these rows sample are glyph ink, grade 3. A glyph sample is a shape, not a text character.
function inkSample(s) {
  const dot = { display: "inline-block", width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "var(--tasty-radius-pill)" };
  if (s === "●") return <span style={{ ...dot, background: "currentColor" }} />;
  if (s === "○") return <span style={{ ...dot, boxSizing: "border-box", border: "var(--tasty-border-width) solid currentColor" }} />;
  if (s === "★") return <CIcon name="starFill" size="var(--tasty-icon-size-sm)" />;
  if (s === "✓") return <CIcon name="check" size="var(--tasty-icon-size-sm)" />;
  return s;
}
