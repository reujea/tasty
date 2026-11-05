import React from "react";
import { Input as FhInput, Select as FhSelect, Button as FhButton } from "../../components/index.js";
import { FileDetectorsPane as Kit_FileDetectorsPane, FileHandlersPane as Kit_FileHandlersPane } from "../../ui_kits/terminal/overlays/file_handlers.jsx";
import { Section, Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// 2026-10-21 follow-up: several sources, user-only Remove, patch on existing id.
// Tasty Gallery — Overlays · Windows › Settings › Handler › File Detectors · File Handlers (2026-10-20).
// Static states of ui_kits/terminal/overlays/file_handlers.jsx
// (the kit panes, pinned through their initial-state props).




const FhPair = ({ children }) => (
  <>{[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
    <div key={label} {...(t ? { "data-theme": t } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)", minWidth: 0, flex: "1 1 var(--tasty-size-560)", maxWidth: "100%" }}>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>{children}
    </div>
  ))}</>
);
// settings content column: same panel ground + the width other Handler subtabs get
const FhPanel = ({ children, narrow }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)",
    border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", width: narrow ? "var(--tasty-size-360)" : "100%", maxWidth: "100%", boxSizing: "border-box" }}>{children}</div>
);
const st = { padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" };
const Pane = ({ k, narrow, ...p }) => {
  const C = k === "det" ? Kit_FileDetectorsPane : Kit_FileHandlersPane;
  return <FhPair><FhPanel narrow={narrow}><C {...p} /></FhPanel></FhPair>;
};

// 2026-10-21 — Input / Select / ghost Button on the raised add card, every state side by side
const fhStateLbl = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-file-handler-form-label-fg)" };
function FhRaisedStates() {
  const cell = (label, node) => <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0 }}><span style={fhStateLbl}>{label}</span>{node}</div>;
  return (
    <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(var(--tasty-size-150), 1fr))", gap: "var(--tasty-space-md)", padding: "var(--tasty-file-handler-form-padding)",
      background: "var(--tasty-file-handler-form-bg)", border: "var(--tasty-border-width) solid var(--tasty-file-handler-form-border)", borderRadius: "var(--tasty-radius)" }}>
      {cell("Input · normal", <FhInput block mono ground="raised" placeholder="my-detector" />)}
      {cell("Input · value", <FhInput block mono ground="raised" defaultValue="dockerfile" />)}
      {cell("Input · invalid", <FhInput block mono ground="raised" invalid defaultValue="Docker_File" />)}
      {cell("Input · disabled", <FhInput block mono ground="raised" disabled defaultValue="dockerfile" />)}
      {cell("Select · placeholder", <FhSelect block ground="raised" placeholder="Select a detector…" options={["markdown", "image"]} value="" onChange={() => {}} />)}
      {cell("Select · value", <FhSelect block ground="raised" options={["markdown", "image"]} defaultValue="markdown" />)}
      {cell("Select · disabled", <FhSelect block ground="raised" disabled options={["markdown", "image"]} defaultValue="markdown" />)}
      {cell("Ghost sm · raised", <span style={{ display: "flex" }}><FhButton variant="ghost" size="sm" ground="raised">Cancel</FhButton></span>)}
    </div>
  );
}
const CONTRAST_SPECS = [
  ["error line", "file-handler-form-error-fg → accent-danger-raised: Mocha = accent-danger 5.43:1 on surface0 · Latte = color-mix(red 60%, neutral-1100) 4.79:1 (was 3.52). Glyph takes the same ink (≥ 3:1)"],
  ["invalid edge", "unchanged — input-border-invalid → accent-danger (non-text, 3.52 ≥ 3 in Latte)"],
  ["Cancel", "Button ground=\"raised\" → button-ghost-raised-fg → text-secondary-raised: Mocha 7.10 · Latte 5.17 (was 4.05) · hover button-ghost-raised-fg-hover → text-primary"],
  ["Input / Select box", "ground=\"raised\" in every card field: bg → bg-panel, edge → border-strong (input-raised-* · select-raised-*). Focus: edge border-focus (Input keeps its ring rule). Invalid: accent-danger edge. Disabled: state-disabled-fill-raised + state-disabled-border-raised. Ink per state unchanged"],
  ["scope", "same fix on the Hook Handlers add card (labels text-secondary-raised, Inputs raised, Cancel raised) · card background unchanged (surface-raised)"],
];
const NARROW_SPECS = [
  ["line 1 fit order", "1 origin Tags collapse → “N sources” · 2 pending Tag leaves line 1 · 3 id ellipsizes — each step only while the id would ellipsize; undone once the line is back to the width it had when the step was taken"],
  ["id floor", "min(file-handler-id-min-width 96, full id width) once the pending Tag has dropped · tooltip = full id"],
  ["dropped pending Tag", "own line between line 1 and the summary, gap space-xs · neutral Tag · max-width 100%, ellipsis, tooltip = full text"],
  ["never shrink", "Switch + trailing slot (Remove / Undo / lock) · prio"],
];
const ROW_SPECS = [["row", "2 lines · padding space-sm space-xs · gap space-xs · bottom 1px separator"], ["line 1", "min-height button-height-sm · gap space-sm · id mono body 13 semibold (ellipsis, full id in tooltip) · origin Tag · [prio N micro] · [pending Tag] · spacer · Switch · trailing slot"],
  ["origin Tag(s)", "host / user neutral · plugin:<id> agent variant · detectors: one Tag per source, order host → plugin → user, gap space-sm; a rule-less user patch shows “user override” · collapse → one neutral “N sources” Tag (tooltip = full list) once the Tags would ellipsize the id"], ["trailing slot", "any user contribution → Remove (ghost sm) · pending → Undo (ghost sm), same slot · no user contribution → lock glyph icon-size-sm, glyph-dim, tooltip “Provided by host, plugin:logkit”"],
  ["line 2", "summary mono term-sm, one line, ellipsis, full value in tooltip · detectors: every source's rules in merge order “ext md, markdown · ext mdx” · handlers “<detector> → <action>”"],
  ["switched off", "id text-secondary · summary text-muted (still editable)"], ["sort", "detectors: registry order · handlers: run order — priority ↑ → owner user > plugin > host → id (2026-10-21)"]];
const CARD_SPECS = [["card", "file-handler-form-bg · 1px file-handler-form-border · radius · padding file-handler-form-padding 12 · row gap file-handler-form-row-gap 8"],
  ["title", "mono micro caps, file-handler-form-label-fg"], ["field row", "label column settings-label-width 150 (wraps, body 13) · gap settings-label-gap 16 · control column flex ≥ field-width-lg; wraps under the label when the card is narrower"],
  ["control column", "control (control-height 28) + one line under it, gap space-xs — priority error > info > caption"], ["info line", "info glyph icon-size-sm + caption 12, file-handler-form-info-fg, role status; same box as the error line; live while typing (not only after Add)"], ["widths", "text Inputs block (fill) · Detector Select file-handler-select-width 200 · Action Select file-handler-kind-width 160 · Priority Input file-handler-priority-width 90"],
  ["buttons", "right-aligned, gap space-sm: Cancel ghost sm · Add primary sm"], ["error", "alertCircle icon-size-sm + caption 12, file-handler-form-error-fg; replaces that field's caption; Input invalid. Cross-field error (no rule) sits above the button row"]];
const TOKENS = [
  { tok: "--tasty-file-handler-form-bg", use: "→ surface-raised", color: "var(--tasty-file-handler-form-bg)" },
  { tok: "--tasty-file-handler-form-border", use: "→ border-default", color: "var(--tasty-file-handler-form-border)" },
  { tok: "--tasty-file-handler-form-label-fg", use: "→ text-secondary-raised", color: "var(--tasty-file-handler-form-label-fg)" },
  { tok: "--tasty-file-handler-form-caption-fg", use: "→ text-secondary-raised", color: "var(--tasty-file-handler-form-caption-fg)" },
  { tok: "--tasty-file-handler-form-error-fg", use: "→ accent-danger-raised (2026-10-21)", color: "var(--tasty-file-handler-form-error-fg)" },
  { tok: "--tasty-file-handler-priority-width", use: "→ field-width-xs 90" },
  { tok: "--tasty-file-handler-select-width", use: "→ field-width-lg 200" },
  { tok: "--tasty-file-handler-kind-width", use: "→ field-width-md 160" },
  { tok: "--tasty-file-handler-prio-fg", use: "→ text-muted", color: "var(--tasty-file-handler-prio-fg)" },
  { tok: "--tasty-file-handler-summary-fg", use: "→ text-secondary", color: "var(--tasty-file-handler-summary-fg)" },
  { tok: "--tasty-file-handler-empty-fg", use: "→ text-muted", color: "var(--tasty-file-handler-empty-fg)" },
    { tok: "--tasty-file-handler-form-info-fg", use: "new · → text-secondary-raised", color: "var(--tasty-file-handler-form-info-fg)" },
  { tok: "--tasty-file-handler-summary-removed-fg", use: "new · → text-muted", color: "var(--tasty-file-handler-summary-removed-fg)" },
];

function FileHandlerSection() {
  return (
    <Section id="filehandlers" title="Handler › File Detectors · File Handlers — list + add card">
      <Spec id="registered-rows-two-lines-no-table" title="Registered rows — two lines, no table"
        when={<>Both subtabs list every registered entry as a <b>two-line row</b> (the Hook Handlers row). Line 1 carries identity and state: <b>id</b>, <b>origin</b> Tag, <b>prio</b> (handlers), Switch and the trailing slot. Line 2 is one mono summary: the detector's <b>rules</b>, or the handler's <b>detector → action</b>. Seven columns do not fit the settings content width, and a table would truncate every column; two lines truncate only the id and the summary, both with the full value in a tooltip. Only <b>user</b> rows have Remove; host and plugin rows show the padlock.</>}>
        <Stage variant="solo" style={st}><Pane k="det" /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" /></Stage>
        <Meta specs={ROW_SPECS} tokens={TOKENS.slice(8, 10)} />
      </Spec>

      <Spec id="add-card-open-empty" title="Add card — open, empty"
        when={<>“Add detector” / “Add handler” (secondary sm, plus glyph, right of the intro) opens the card between the intro and the list, and is disabled while it is open. Add stays <b>disabled until the required fields have a value</b> (Hook Handlers rule): detector = Id; handler = Short name + Detector + the action's required field. The <b>Saved as user/…</b> caption shows the stored id as you type.</>}>
        <Stage variant="solo" style={st}><Pane k="det" adding /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" adding /></Stage>
        <Meta specs={CARD_SPECS} tokens={TOKENS.slice(0, 8)} />
      </Spec>

      <Spec id="add-card-typing-validation-errors" title="Add card — typing, validation errors"
        when={<>Format errors are checked <b>when Add is pressed</b> (Add was enabled: the required fields are filled). The error replaces the field's caption and stays until the field is valid again; Add stays clickable. Detector: <b>id characters</b> (lowercase, digits, -, ≤ 64) and the cross-field <b>no extension and no glob</b> line above the buttons. Handler: <b>short name characters</b> (≤ 32), <b>priority not a whole number</b>, IPC <b>method not dotted</b>. There is <b>no duplicate error</b> any more: an existing id is a patch (below). The detector Id caption is the character rule; the stored id equals the input, so there is no “Saved as” line. Handlers keep “Saved as user/&lt;short&gt;”.</>}>
        <Stage variant="solo" style={st}><Pane k="det" adding attempted draft={{ id: "Docker_File" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" adding attempted draft={{ short: "docker.system", det: "dockerfile", prio: "1.5", surface: "text_view" }} /></Stage>
        <Note>Strings: <code>file_detector.id.hint</code> “Lowercase letters, digits and -. Up to 64.” · <code>file_detector.error.id_chars</code> “Use lowercase letters, digits and - only, up to 64.” · <code>file_detector.error.reserved</code> “Ids starting with $ are reserved. You can only change ones that already exist.” · <code>.error.no_rules</code> “Add at least one extension or a path glob.” · <code>file_handler.error.short_chars</code> “Use lowercase letters, digits and - only, up to 32.” · <code>.error.priority_int</code> “Enter a whole number.” · <code>.error.method</code> “Use a dotted name, e.g. com.example.foo.open.” Removed: <code>.error.duplicate</code>.</Note>
      </Spec>

      <Spec id="handler-add-card-action-kinds" title="Handler add card — action kinds"
        when={<>The Action Select swaps the rows below it. <b>Open surface</b>: Surface kind (required) + Param key (empty = <code>file</code>). <b>IPC</b>: Method (required). <b>System</b>: no rows; the Action caption says what it does.</>}>
        <Stage variant="solo" style={st}><Pane k="hdl" adding draft={{ short: "notes-view", det: "markdown", kind: "surface", surface: "markdown_view" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" adding draft={{ short: "notes-ipc", det: "markdown", prio: "40", kind: "ipc", method: "com.example.notes.open" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" adding draft={{ short: "notes-os", det: "markdown", kind: "system" }} /></Stage>
      </Spec>

      <Spec id="several-sources-on-one-detector" title="Several sources on one detector"
        when={<>A detector id merges contributions <b>Host → Plugin → User</b>. Line 1 shows <b>one origin Tag per source</b> in that order; line 2 lists every source's rules in the same order. A user patch with <b>no rules</b> (e.g. the user only switched a plugin detector off) adds a <b>user override</b> Tag. Any user contribution puts <b>Remove</b> in the trailing slot even when other sources exist; rows with no user contribution keep the padlock. When the Tags would push the id into an ellipsis (narrow column, long German strings) they collapse into one <b>N sources</b> Tag whose tooltip lists them; the id keeps its room.</>}>
        <Stage variant="solo" style={st}><Pane k="det" /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" narrow /></Stage>
        <Note>Rows: <code>markdown</code> host + user rule · <code>log</code> host + plugin:logkit + user rule · <code>notebook</code> plugin:md-plus + user override (switched off, no user rule). Strings: <code>file_detector.origin.user_override</code> “user override” · <code>.origin.sources</code> “{"{n}"} sources”.</Note>
      </Spec>

      <Spec id="remove-a-user-contribution-the-row-stays" title="Remove a user contribution — the row stays"
        when={<>On a row with other sources, Remove takes out <b>only the user part</b>. The row is <b>not struck</b>: only the <b>user Tag</b> is struck, in its normal ink, the user rule pieces in line 2 are struck in <code>summary-removed-fg</code>, and the pending Tag says what goes: <b>your rules removed on save</b>, or <b>your override removed on save</b> for a rule-less patch. The Switch is disabled and shows the state the row will have <b>after Save</b> (the plugin default for <code>notebook</code>). Undo restores the row. A row with <b>only</b> a user contribution keeps the old idiom: whole id struck, <b>removed on save</b>.</>}>
        <Stage variant="solo" style={st}><Pane k="det" unpatch /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" unpatch pending /></Stage>
        <Note>Strings: <code>file_detector.pending.yours_removed</code> “your rules removed on save” · <code>.pending.override_removed</code> “your override removed on save” · <code>.pending.removed</code> “removed on save”.</Note>
      </Spec>

      <Spec id="add-with-an-existing-id-patch-not-an-error" title="Add with an existing id — patch, not an error"
        when={<>Typing an id that is already registered shows an <b>info line</b> under the field at once (before Add), and the primary button reads <b>Update detector</b>. No rule changes for Add being enabled. With no user contribution yet: “<b>{"{id}"} already exists. Your rules and settings are added to it on save.</b>” With an earlier user contribution: “<b>You already changed {"{id}"}. This replaces your earlier rules and settings on save.</b>” A registered reserved id (<code>$directory</code>) is patched the same way. A <b>new</b> reserved id (<code>$foo</code>) passes the character check but fails with the reserved error on Add. After Update the card closes and the <b>existing row</b> gets a user Tag and <b>your rules added on save</b> / <b>your rules replaced on save</b>; no new row appears. Undo restores the previous contribution.</>}>
        <Stage variant="solo" style={st}><Pane k="det" adding draft={{ id: "image", ext: "heic" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" adding draft={{ id: "markdown", ext: "mdx, mdown" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" adding draft={{ id: "$directory", glob: "**/.git" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" adding attempted draft={{ id: "$foo", ext: "foo" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" patched /></Stage>
        <Note>Strings: <code>file_detector.info.patch</code> · <code>.info.replace</code> · <code>.add.update</code> “Update detector” · <code>.pending.yours_added</code> “your rules added on save” · <code>.pending.yours_replaced</code> “your rules replaced on save”.</Note>
      </Spec>

      <Spec id="handler-same-short-name-replaces" title="Handler — same short name replaces"
        when={<>A short name whose <code>user/&lt;short&gt;</code> already exists shows the info line “<b>user/{"{short}"} already exists. This replaces it on save.</b>” in place of the Saved as caption, and the button reads <b>Update handler</b>. After Update the row shows the new values with <b>replaced on save</b> + Undo (back to the old values). Host / plugin handlers never collide: their ids have no <code>user/</code> prefix.</>}>
        <Stage variant="solo" style={st}><Pane k="hdl" adding draft={{ short: "dockerfile-system", det: "dockerfile", kind: "system" }} /></Stage>
        <Note>Strings: <code>file_handler.info.replace</code> · <code>.add.update</code> “Update handler” · <code>.pending.replaced</code> “replaced on save”.</Note>
      </Spec>

      <Spec id="handler-run-order-the-list-is-the-order-handlers-run" title="Handler run order — the list is the order handlers run"
        when={<>A detector with several handlers runs the first one by <b>priority ↑</b>, then owner <b>user &gt; plugin &gt; host</b>, then <b>id</b>. That contract is fixed; the list now <b>sorts by it</b>, so reading a detector's rows top to bottom is reading the run order. Sorting by id alone put plugin ids (<code>com.…/</code>) before <code>host/</code> before <code>user/</code> at a tie, the reverse of what runs. The intro says the rule and that the list follows it. Pending rows (added · replaced · removed) take the same slot rule.</>}>
        <Stage variant="solo" style={st}><Pane k="hdl" tie /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" tie lang="de" narrow /></Stage>
        <Note>Scene: detector <code>markdown</code> at priority 10 with <code>user/my-viewer</code> · <code>com.example.md/preview</code> · <code>host/markdown-view</code> — rows read in that order (host/image-view, another detector, sorts after them at the same priority). Strings: <code>file_handler.intro</code> “A handler opens a file that a detector recognized. Lower priority runs first. On a tie, user handlers run before plugin handlers, and plugin handlers before host ones. The list is in run order.” (de checked for length; ko/ja by the implementer).</Note>
      </Spec>

      <Spec id="add-card-on-a-raised-ground-error-ink-cancel-field-edges" title="Add card on a raised ground — error ink · Cancel · field edges"
        when={<>The add card is <b>surface-raised</b>. Three things on it fell under 4.5:1 or vanished in Latte: the <b>error lines</b> (accent-danger 3.52:1), the <b>Cancel</b> ghost label (text-secondary 4.05:1) and the <b>normal box</b> of Input and Select (border-default equals surface-raised in both themes). The card ground stays; the parts on it switch to their <b>raised</b> roles: a new semantic <code>accent-danger-raised</code> for error ink, <code>Button ground="raised"</code> for ghost, and <code>ground="raised"</code> on every Input and Select (Select gains the same variant Input has had since 2026-10-14).</>}>
        <Stage variant="solo" style={st}><Pane k="det" adding attempted draft={{ id: "Docker_File" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" adding attempted draft={{ short: "Bad.Name", det: "dockerfile", prio: "1.5", surface: "text_view" }} /></Stage>
        <Stage variant="solo" style={st}><FhPair><FhRaisedStates /></FhPair></Stage>
        <Meta specs={CONTRAST_SPECS} tokens={[
          { tok: "--tasty-accent-danger-raised", use: "new semantic · Mocha → accent-danger · Latte → color-mix(red 60%, n1100)", color: "var(--tasty-accent-danger-raised)" },
          { tok: "--tasty-button-ghost-raised-fg", use: "new · → text-secondary-raised", color: "var(--tasty-button-ghost-raised-fg)" },
          { tok: "--tasty-button-ghost-raised-fg-hover", use: "new · → text-primary", color: "var(--tasty-button-ghost-raised-fg-hover)" },
          { tok: "--tasty-select-raised-bg", use: "new · → bg-panel", color: "var(--tasty-select-raised-bg)" },
          { tok: "--tasty-select-raised-border", use: "new · → border-strong", color: "var(--tasty-select-raised-border)" },
          { tok: "--tasty-select-raised-disabled-bg", use: "new · → state-disabled-fill-raised", color: "var(--tasty-select-raised-disabled-bg)" },
          { tok: "--tasty-select-raised-disabled-border", use: "new · → state-disabled-border-raised", color: "var(--tasty-select-raised-disabled-border)" }]} />
      </Spec>

      <Spec id="narrow-column-long-labels-pending-rows-keep-the-id" title="Narrow column · long labels — pending rows keep the id"
        when={<>At <code>size-360</code> in German, “N Quellen” plus a long pending Tag used to squeeze the id to one ellipsis character and push Switch / Undo off the row. Line 1 now gives room up in a fixed order: the origin Tags <b>collapse</b> first; if the id would still ellipsize, the <b>pending Tag moves to its own line</b> under line 1 (ellipsis + tooltip if even that line is short); only then does the id ellipsize, and never below <b>96 px</b> (or its full width if shorter). Switch and the trailing slot never shrink. Wide columns keep the one-line layout.</>}>
        <Stage variant="solo" style={st}><Pane k="det" lang="de" narrow narrowSet /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" lang="de" narrowSet /></Stage>
        <Meta specs={NARROW_SPECS} tokens={[{ tok: "--tasty-file-handler-id-min-width", use: "new · → size-96" }]} />
        <Note>Rows: <code>markdown</code> host + user “eigene Regeln beim Speichern entfernt” · <code>notebook</code> plugin:md-plus + user override “eigene Änderung beim Speichern entfernt” · <code>log</code> host + plugin:logkit + user “eigene Regeln beim Speichern hinzugefügt”. No new strings.</Note>
      </Spec>

      <Spec id="pending-added-on-save-removed-on-save" title="Pending — added on save · removed on save"
        when={<>Nothing is written until the Settings <b>Save</b>; Cancel drops it. Add <b>closes the card</b> and puts the entry <b>in the list at its sort position</b> with a neutral Tag <b>added on save</b>; Undo (same slot as Remove) takes it out. Remove keeps the row, strikes the id, shows <b>removed on save</b>, disables its Switch and turns Remove into Undo. Same pair as File Extension Mapping and Hook Handlers.</>}>
        <Stage variant="solo" style={st}><Pane k="det" pending /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" pending /></Stage>
      </Spec>

      <Spec id="empty-list" title="Empty list"
        when={<>One row-height line in caption <b>text-muted</b> where the rows would be, with the same bottom separator. The header count reads (0); the Add button is unchanged.</>}>
        <Stage variant="solo" style={st}><Pane k="det" empty /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" empty /></Stage>
      </Spec>

      <Spec id="long-labels-narrow-column" title="Long labels · narrow column"
        when={<>Labels wrap inside the 150 column (German). When the card is narrower than label + gap + field-width-lg the control drops under its label. Long ids and summaries ellipsize on one line.</>}>
        <Stage variant="solo" style={st}><Pane k="hdl" lang="de" adding draft={{ short: "notizen", det: "markdown", kind: "surface" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="hdl" narrow adding draft={{ short: "notes-view", det: "markdown", kind: "ipc", method: "com.example.notes.open" }} /></Stage>
        <Stage variant="solo" style={st}><Pane k="det" lang="de" narrow unpatch /></Stage>
        <Do>Keep the summary on one line and put the full value in the tooltip, so every row has the same height.</Do>
        <Dont>Shrink the label column below settings-label-width to fit long labels; wrap them instead.</Dont>
      </Spec>
    </Section>
  );
}

export { FileHandlerSection };
