import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon as FhIcon } from "../../../components/core/Icon.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { Input } from "../../../components/forms/Input.jsx";
import { Select } from "../../../components/forms/Select.jsx";
import { Switch } from "../../../components/forms/Switch.jsx";

// Tasty UI kit — Settings › Handler › File Detectors · File Handlers (2026-10-20;
// 2026-10-21 follow-up: unprefixed detector ids, char rules,
// 2026-10-21 contract + contrast: run-order sort, raised-ground
// controls in the add card, line-1 fit order for narrow pending rows.
// several sources per detector, existing id = patch / replace, Remove drops only the user contribution).
// Registered list (2-line rows) + user add card + Save-pending states with Undo (File Extension Mapping idiom).
// Initial-state props exist only so the gallery can pin each state; the live kit passes none.




const FH_COPY = {
  en: {
    detIntro: "A detector decides what kind of file a path is. Enabled detectors are tried in order; the first whose rules match wins.",
    hdlIntro: "A handler opens a file that a detector recognized. Lower priority runs first. On a tie, user handlers run before plugin handlers, and plugin handlers before host ones. The list is in run order.",
    bullets: [["host", "built in"], ["plugin:<id>", "added by a plugin"], ["user", "yours — only these can be removed"]],
    addDet: "Add detector", addHdl: "Add handler", updDet: "Update detector", updHdl: "Update handler", newDet: "New detector", newHdl: "New handler",
    regDet: "Registered detectors", regHdl: "Registered handlers",
    emptyDet: "No detectors registered.", emptyHdl: "No handlers registered.",
    id: "Id", ext: "Extensions", glob: "Path glob", short: "Short name", detector: "Detector", priority: "Priority",
    action: "Action", surface: "Surface kind", param: "Param key", method: "Method",
    savedAs: "Saved as", detIdHint: "Lowercase letters, digits and -. Up to 64.",
    prioHint: "Lower runs first. Empty = 100.", paramHint: "Empty = file.", extHint: "Comma-separated, no dot.",
    systemHint: "Opens the file with the OS default app. No other settings.",
    pickDet: "Select a detector…", kinds: { surface: "Open surface", ipc: "IPC", system: "System" },
    cancel: "Cancel", undo: "Undo", remove: "Remove",
    added: "added on save", removed: "removed on save", replaced: "replaced on save",
    yoursAdded: "your rules added on save", yoursReplaced: "your rules replaced on save",
    yoursRemoved: "your rules removed on save", overrideRemoved: "your override removed on save",
    userOverride: "user override", sources: (n) => n + " sources",
    locked: (o) => "Provided by " + o + " — can't be removed",
    infoPatch: (id) => id + " already exists. Your rules and settings are added to it on save.",
    infoReplaceDet: (id) => "You already changed " + id + ". This replaces your earlier rules and settings on save.",
    infoReplaceHdl: (id) => id + " already exists. This replaces it on save.",
    errDetChars: "Use lowercase letters, digits and - only, up to 64.",
    errHdlChars: "Use lowercase letters, digits and - only, up to 32.",
    errReserved: "Ids starting with $ are reserved. You can only change ones that already exist.",
    errRules: "Add at least one extension or a path glob.", errInt: "Enter a whole number.",
    errMethod: "Use a dotted name, e.g. com.example.foo.open.",
  },
  de: {
    detIntro: "Ein Detektor bestimmt, welche Art Datei ein Pfad ist. Aktive Detektoren werden der Reihe nach geprüft; der erste mit passenden Regeln gewinnt.",
    hdlIntro: "Ein Handler öffnet eine Datei, die ein Detektor erkannt hat. Niedrigere Priorität läuft zuerst. Bei Gleichstand laufen user-Handler vor plugin-Handlern und plugin-Handler vor host-Handlern. Die Liste steht in Ausführungsreihenfolge.",
    bullets: [["host", "eingebaut"], ["plugin:<id>", "von einem Plugin"], ["user", "eigene — nur diese lassen sich entfernen"]],
    addDet: "Detektor hinzufügen", addHdl: "Handler hinzufügen", updDet: "Detektor aktualisieren", updHdl: "Handler aktualisieren", newDet: "Neuer Detektor", newHdl: "Neuer Handler",
    regDet: "Registrierte Detektoren", regHdl: "Registrierte Handler",
    emptyDet: "Keine Detektoren registriert.", emptyHdl: "Keine Handler registriert.",
    id: "Id", ext: "Dateiendungen", glob: "Pfad-Glob", short: "Kurzname", detector: "Detektor", priority: "Priorität",
    action: "Aktion", surface: "Oberflächentyp", param: "Parameterschlüssel", method: "Methode",
    savedAs: "Gespeichert als", detIdHint: "Kleinbuchstaben, Ziffern und -. Höchstens 64.",
    prioHint: "Niedriger läuft zuerst. Leer = 100.", paramHint: "Leer = file.", extHint: "Kommagetrennt, ohne Punkt.",
    systemHint: "Öffnet die Datei mit der Standard-App des Systems. Keine weiteren Einstellungen.",
    pickDet: "Detektor auswählen…", kinds: { surface: "Oberfläche öffnen", ipc: "IPC", system: "System" },
    cancel: "Abbrechen", undo: "Rückgängig", remove: "Entfernen",
    added: "beim Speichern hinzugefügt", removed: "beim Speichern entfernt", replaced: "beim Speichern ersetzt",
    yoursAdded: "eigene Regeln beim Speichern hinzugefügt", yoursReplaced: "eigene Regeln beim Speichern ersetzt",
    yoursRemoved: "eigene Regeln beim Speichern entfernt", overrideRemoved: "eigene Änderung beim Speichern entfernt",
    userOverride: "user-Änderung", sources: (n) => n + " Quellen",
    locked: (o) => "Von " + o + " bereitgestellt — nicht entfernbar",
    infoPatch: (id) => id + " existiert bereits. Deine Regeln und Einstellungen werden beim Speichern ergänzt.",
    infoReplaceDet: (id) => "Du hast " + id + " bereits geändert. Das ersetzt beim Speichern deine früheren Regeln und Einstellungen.",
    infoReplaceHdl: (id) => id + " existiert bereits. Wird beim Speichern ersetzt.",
    errDetChars: "Nur Kleinbuchstaben, Ziffern und - verwenden, höchstens 64.",
    errHdlChars: "Nur Kleinbuchstaben, Ziffern und - verwenden, höchstens 32.",
    errReserved: "Ids mit $ am Anfang sind reserviert. Nur vorhandene lassen sich ändern.",
    errRules: "Mindestens eine Endung oder einen Pfad-Glob angeben.", errInt: "Ganze Zahl eingeben.",
    errMethod: "Punktnotation verwenden, z. B. com.example.foo.open.",
  },
};

// Detector = merged contributions (Host → Plugin → User). rules[].src is each rule's source; userMeta = a user
// patch with no rules (e.g. only switched off). baseOn = the state without the user contribution.
const DET_SEED = [
  { id: "markdown", rules: [{ src: "host", ext: ["md", "markdown"] }, { src: "user", ext: ["mdx"] }], on: true },
  { id: "image", rules: [{ src: "host", ext: ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "tiff", "avif"] }], on: true },
  { id: "log", rules: [{ src: "host", ext: ["log"] }, { src: "plugin:logkit", glob: "*.log.[0-9]*" }, { src: "user", glob: "/var/log/**/*.out" }], on: true },
  { id: "$directory", rules: [{ src: "host", text: "any directory" }], on: true },
  { id: "notebook", rules: [{ src: "plugin:md-plus", ext: ["ipynb"] }], on: false, baseOn: true, userMeta: true },
  { id: "dockerfile", rules: [{ src: "user", glob: "Dockerfile" }], on: true },
];
// 2026-10-21 — real HandlerIds carry the owner prefix: host/<name> · <plugin_id>/<name> · user/<name>
const HDL_SEED = [
  { id: "host/image-view", prio: 10, origin: "host", det: "image", act: "surface:image_view", on: true },
  { id: "host/markdown-view", prio: 10, origin: "host", det: "markdown", act: "surface:markdown_view", on: true },
  { id: "host/log-view", prio: 20, origin: "host", det: "log", act: "surface:log_view", on: true },
  { id: "md-plus/notebook-open", prio: 50, origin: "plugin:md-plus", det: "notebook", act: "ipc:com.mdplus.notebook.open_in_preview_surface", on: false },
  { id: "user/dockerfile-system", prio: 100, origin: "user", det: "dockerfile", act: "system", on: true },
];
const PENDING_DET = [{ id: "notes", rules: [{ src: "user", ext: ["note", "notes"] }], on: true, pending: "add" }];
const PENDING_HDL = [{ id: "user/notes-view", prio: 30, origin: "user", det: "notes", act: "surface:markdown_view", on: true, pending: "add" }];
// same detector + same priority: one handler per owner (gallery "tie" scene)
const TIE_HDL = [
  { id: "com.example.md/preview", prio: 10, origin: "plugin:com.example.md", det: "markdown", act: "surface:md_preview", on: true },
  { id: "user/my-viewer", prio: 10, origin: "user", det: "markdown", act: "system", on: true },
];
// 2026-10-21 — list order = run order (product contract): priority ↑ → owner user > plugin > host → id.
// Plain id order would read plugin (com.…/) → host → user at a tie, the reverse of what runs.
const OWNER_RANK = (o) => (o === "user" ? 0 : o.startsWith("plugin:") ? 1 : 2);
const sortHdl = (xs) => xs.slice().sort((a, b) => a.prio - b.prio || OWNER_RANK(a.origin) - OWNER_RANK(b.origin) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
const DET_RE = /^[a-z0-9-]{1,64}$/, DET_RES_RE = /^\$[a-z0-9-]{1,64}$/, HDL_RE = /^[a-z0-9-]{1,32}$/;

const userRules = (d) => d.rules.filter((r) => r.src === "user");
const hasUser = (d) => userRules(d).length > 0 || !!d.userMeta;
const otherSrcs = (d) => [...new Set(d.rules.map((r) => r.src).filter((s) => s !== "user"))];
const strip = (x) => { const { pending, prev, ...rest } = x; return rest; };

const fhHead = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase",
  letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" };
const fhCardHead = { ...fhHead, color: "var(--tasty-file-handler-form-label-fg)" };
const fhEll = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" };

function FhIntro({ L, text, addLabel, onAdd, adding }) {
  const b = { color: "var(--tasty-text-secondary)", fontFamily: "var(--tasty-font-mono)" };
  return (
    <div style={{ display: "flex", alignItems: "flex-start", flexWrap: "wrap", gap: "var(--tasty-space-md)" }}>
      <div style={{ flex: "1 1 var(--tasty-size-240)", minWidth: 0, maxWidth: "var(--tasty-measure-md)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)",
        fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>
        <p style={{ margin: 0 }}>{text}</p>
        <ul style={{ margin: 0, paddingLeft: "var(--tasty-space-lg)", display: "flex", flexDirection: "column" }}>
          {L.bullets.map(([k, v]) => <li key={k}><b style={b}>{k}</b> — {v}</li>)}
        </ul>
      </div>
      <span style={{ flex: "none" }}><Button variant="secondary" size="sm" leadingIcon={<FhIcon name="plus" />} disabled={adding} onClick={onAdd}>{addLabel}</Button></span>
    </div>
  );
}

// one card row: label column (settings-label-width, wraps) · control column (control + ONE line: error > info > caption)
function FhField({ label, children, caption, info, error }) {
  return (
    <div style={{ display: "flex", alignItems: "flex-start", flexWrap: "wrap", columnGap: "var(--tasty-settings-label-gap)", rowGap: "var(--tasty-space-xs)" }}>
      <span style={{ flex: "none", width: "var(--tasty-settings-label-width)", minHeight: "var(--tasty-control-height)", display: "flex", alignItems: "center",
        fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-file-handler-form-label-fg)", lineHeight: "var(--tasty-line-height-ui)" }}>{label}</span>
      <div style={{ flex: "1 1 var(--tasty-field-width-lg)", minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
        {children}
        {error ? <FhLine kind="error">{error}</FhLine> : info ? <FhLine kind="info">{info}</FhLine>
          : caption ? <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-file-handler-form-caption-fg)", lineHeight: "var(--tasty-line-height-ui)", overflowWrap: "anywhere" }}>{caption}</span> : null}
      </div>
    </div>
  );
}
// error: alertCircle + form-error-fg (role alert) · info: info glyph + form-info-fg (role status) — same box
function FhLine({ kind, children }) {
  const err = kind === "error";
  return (
    <span role={err ? "alert" : "status"} style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)",
      color: err ? "var(--tasty-file-handler-form-error-fg)" : "var(--tasty-file-handler-form-info-fg)" }}>
      <span style={{ flex: "none", display: "inline-flex", height: "calc(var(--tasty-font-size-caption) * var(--tasty-line-height-ui))", alignItems: "center" }}><FhIcon name={err ? "alertCircle" : "info"} size="var(--tasty-icon-size-sm)" /></span>
      <span style={{ minWidth: 0, overflowWrap: "anywhere" }}>{children}</span>
    </span>
  );
}
const FhError = ({ children }) => <FhLine kind="error">{children}</FhLine>;
function FhCard({ title, children, error, onCancel, onAdd, addLabel, canAdd, L }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-file-handler-form-row-gap)", padding: "var(--tasty-file-handler-form-padding)",
      border: "var(--tasty-border-width) solid var(--tasty-file-handler-form-border)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-file-handler-form-bg)" }}>
      <span style={fhCardHead}>{title}</span>
      {children}
      {error && <FhError>{error}</FhError>}
      <div style={{ display: "flex", justifyContent: "flex-end", flexWrap: "wrap", gap: "var(--tasty-space-sm)" }}>
        <Button variant="ghost" size="sm" ground="raised" onClick={onCancel}>{L.cancel}</Button>
        <Button variant="primary" size="sm" disabled={!canAdd} onClick={onAdd}>{addLabel}</Button>
      </div>
    </div>
  );
}

// 2026-10-21 — line 1 fit order when it runs out of room (each step only while the id would ellipsize):
//   1. several origin Tags collapse into ONE "N sources" Tag (tooltip = full list)
//   2. the pending Tag leaves line 1 for its own line under it (ellipsis + tooltip there)
//   3. only then the id ellipsizes, never below min(file-handler-id-min-width 96, its full width)
// Switch + trailing slot never shrink. Each step undoes once the line is as wide as it was when the step was taken.
function useFit(multi, pend) {
  const lineRef = React.useRef(null), idRef = React.useRef(null);
  const [stage, setStage] = React.useState(0);
  const need = React.useRef([0, 0, 0]), idFull = React.useRef(0);
  const max = pend ? 2 : multi ? 1 : 0;
  const up = (s) => (s + 1 === 1 && !multi ? 2 : s + 1), down = (s) => (s - 1 === 1 && !multi ? 0 : s - 1);
  const check = React.useCallback(() => {
    const line = lineRef.current, id = idRef.current;
    if (!line || !id) return;
    setStage((s0) => {
      const s = Math.min(s0, max);
      const over = (id.scrollWidth - id.clientWidth) + (line.scrollWidth - line.clientWidth);
      if (over > 0) { const n = up(s); if (n <= max) { need.current[n] = line.clientWidth + over; idFull.current = id.scrollWidth; return n; } return s; }
      if (s > 0 && line.clientWidth >= need.current[s]) return down(s);
      return s;
    });
  }, [max, multi]);
  React.useLayoutEffect(() => {
    if (!lineRef.current || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(check);
    ro.observe(lineRef.current);
    return () => ro.disconnect();
  }, [check]);
  React.useLayoutEffect(() => { check(); });
  const eff = Math.min(stage, max);
  return [lineRef, idRef, eff >= 1 && multi, eff === 2, idFull.current];
}

// registered row — line 1: id · origin Tag(s) · [prio] · [pending Tag] · spacer · Switch · trailing slot
//                  line 2: summary (mono term-sm, one line, ellipsis + full value as tooltip)
// origins: [{ label, plugin, struck }] · trailing: "lock" | "remove" | "undo"
function FhRow({ L, id, origins, prio, pendingLabel, strikeId, on, switchOn, switchDisabled, trailing, lockedBy, summary, summaryTitle, onToggle, onRemove, onUndo }) {
  const [lineRef, idRef, collapsed, dropped, idFull] = useFit(origins.length > 1, !!pendingLabel);
  const dim = !on || strikeId;
  const tags = collapsed
    ? [<span key="n" title={origins.map((o) => o.label).join(", ")} style={{ flex: "none", display: "inline-flex" }}><Tag>{L.sources(origins.length)}</Tag></span>]
    : origins.map((o) => (
      <span key={o.label} style={{ flex: "none", display: "inline-flex" }}>
        <Tag variant={o.plugin ? "agent" : undefined}>{o.struck ? <s>{o.label}</s> : o.label}</Tag>
      </span>
    ));
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm) var(--tasty-space-xs)",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div ref={lineRef} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0, minHeight: "var(--tasty-button-height-sm)" }}>
        <span ref={idRef} title={id} style={{ flex: "0 1 auto", minWidth: dropped ? "min(var(--tasty-file-handler-id-min-width), " + idFull + "px)" : 0, ...fhEll, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)",
          color: dim ? "var(--tasty-text-secondary)" : "var(--tasty-text-primary)", textDecoration: strikeId ? "line-through" : undefined }}>{id}</span>
        {tags}
        {prio != null && <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-file-handler-prio-fg)" }}>prio {prio}</span>}
        {pendingLabel && !dropped && <span style={{ flex: "none", display: "inline-flex" }}><Tag>{pendingLabel}</Tag></span>}
        <span style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none" }}>
          <Switch checked={switchOn} disabled={switchDisabled} onChange={onToggle} aria-label={"Enable " + id} />
          {trailing === "lock"
            ? <span title={L.locked(lockedBy)} style={{ display: "inline-flex", color: "var(--tasty-glyph-dim)" }}><FhIcon name="lock" size="var(--tasty-icon-size-sm)" /></span>
            : trailing === "undo" ? <Button variant="ghost" size="sm" onClick={onUndo}>{L.undo}</Button>
            : <Button variant="ghost" size="sm" onClick={onRemove}>{L.remove}</Button>}
        </span>
      </div>
      {pendingLabel && dropped && (
        <span style={{ display: "flex", minWidth: 0 }}>
          <Tag title={pendingLabel} style={{ maxWidth: "100%", minWidth: 0 }}><span style={{ ...fhEll, minWidth: 0 }}>{pendingLabel}</span></Tag>
        </span>
      )}
      <span title={summaryTitle} style={{ ...fhEll, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)",
        color: dim ? "var(--tasty-text-muted)" : "var(--tasty-file-handler-summary-fg)" }}>{summary}</span>
    </div>
  );
}
function FhEmpty({ children }) {
  return <div style={{ display: "flex", alignItems: "center", minHeight: "var(--tasty-settings-row-min-height)", padding: "0 var(--tasty-space-xs)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)",
    fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-file-handler-empty-fg)" }}>{children}</div>;
}

const ruleParts = (r) => [r.ext && r.ext.length ? "ext " + r.ext.join(", ") : null, r.glob ? "path " + r.glob : null, r.text || null].filter(Boolean);
function detSummary(d) {
  const segs = d.rules.flatMap((r) => ruleParts(r).map((t) => ({ t, user: r.src === "user" })));
  const struck = d.pending === "unpatch";
  const node = segs.map((s, i) => (
    <React.Fragment key={i}>
      {i > 0 && "  ·  "}
      {struck && s.user ? <s style={{ color: "var(--tasty-file-handler-summary-removed-fg)" }}>{s.t}</s> : s.t}
    </React.Fragment>
  ));
  return [node, segs.map((s) => s.t).join("  ·  ")];
}
const hdlSummary = (h) => h.det + "  →  " + h.act;

function detRowProps(L, x) {
  const others = otherSrcs(x), ur = userRules(x).length > 0, user = hasUser(x), un = x.pending === "unpatch";
  const origins = others.map((s) => ({ label: s, plugin: s.startsWith("plugin:") }));
  if (user) origins.push({ label: ur ? "user" : L.userOverride, struck: un });
  const pendingLabel = x.pending === "add" ? L.added : x.pending === "remove" ? L.removed
    : x.pending === "patch" ? (x.prev && hasUser(x.prev) ? L.yoursReplaced : L.yoursAdded)
    : un ? (ur ? L.yoursRemoved : L.overrideRemoved) : null;
  const [summary, summaryTitle] = detSummary(x);
  return {
    id: x.id, origins, pendingLabel, strikeId: x.pending === "remove", on: x.on && x.pending !== "remove",
    switchOn: un ? (x.baseOn ?? x.on) : x.on, switchDisabled: x.pending === "remove" || un,
    trailing: !user ? "lock" : x.pending ? "undo" : "remove", lockedBy: others.join(", "), summary, summaryTitle,
  };
}

function FileDetectorsPane({ lang = "en", empty = false, pending = false, unpatch = false, patched = false, narrowSet = false, adding: adding0 = false, draft: draft0, attempted = false }) {
  const L = FH_COPY[lang] || FH_COPY.en;
  const init = () => {
    let a = empty ? [] : DET_SEED.map((x) => ({ ...x }));
    if (pending) a = [...a.map((x) => (hasUser(x) && !otherSrcs(x).length ? { ...x, pending: "remove" } : x)), ...PENDING_DET];
    if (unpatch) a = a.map((x) => (hasUser(x) && otherSrcs(x).length ? { ...x, pending: "unpatch" } : x));
    if (patched) a = a.map((x) => (x.id === "image" ? { ...x, rules: [...x.rules, { src: "user", ext: ["heic"] }], pending: "patch", prev: x }
      : x.id === "markdown" ? { ...x, rules: [x.rules[0], { src: "user", ext: ["mdx", "mdown"] }], pending: "patch", prev: x } : x));
    // 2026-10-21 — the three narrow pending rows: markdown (user rules removed) · notebook (override removed) · log (user rules added)
    if (narrowSet) a = a.map((x) => (x.id === "markdown" || x.id === "notebook" ? { ...x, pending: "unpatch" }
      : x.id === "log" ? { ...x, pending: "patch", prev: { ...x, rules: x.rules.filter((r) => r.src !== "user") } } : x));
    return a;
  };
  const [xs, setXs] = React.useState(init);
  const blank = { id: "", ext: "", glob: "" };
  const [adding, setAdding] = React.useState(adding0);
  const [d, setD] = React.useState({ ...blank, ...(draft0 || {}) });
  const [tried, setTried] = React.useState(attempted);
  const set = (k) => (e) => setD((o) => ({ ...o, [k]: e.target.value }));
  const id = d.id.trim();
  const exts = d.ext.split(",").map((s) => s.trim().replace(/^\./, "")).filter(Boolean);
  const exists = id ? xs.find((x) => x.id === id) : null;
  const reserved = id.startsWith("$");
  const errId = !id ? null : reserved ? (!DET_RES_RE.test(id) ? L.errDetChars : !exists ? L.errReserved : null) : !DET_RE.test(id) ? L.errDetChars : null;
  const info = exists && !errId ? (hasUser(exists.pending === "patch" ? exists.prev : exists) || exists.pending === "add" ? L.infoReplaceDet(id) : L.infoPatch(id)) : null;
  const errRules = !exts.length && !d.glob.trim() ? L.errRules : null;
  const close = () => { setAdding(false); setD(blank); setTried(false); };
  const commit = () => {
    setTried(true);
    if (errId || errRules) return;
    const rule = { src: "user", ext: exts, glob: d.glob.trim() };
    if (!exists) setXs((a) => [...a, { id, rules: [rule], on: true, pending: "add" }]);
    else setXs((a) => a.map((x) => {
      if (x.id !== id) return x;
      if (x.pending === "add") return { ...x, rules: [rule] };
      const prev = x.pending ? x.prev || strip(x) : x;
      return { ...strip(x), rules: [...x.rules.filter((r) => r.src !== "user"), rule], userMeta: false, pending: "patch", prev };
    }));
    close();
  };
  const remove = (x) => setXs((a) => a.map((y) => (y.id !== x.id ? y : { ...y, pending: otherSrcs(y).length ? "unpatch" : "remove" })));
  const undo = (x) => setXs((a) => a.flatMap((y) => (y.id !== x.id ? [y] : y.pending === "add" ? [] : y.pending === "patch" ? [y.prev] : [strip(y)])));
  const toggle = (x) => setXs((a) => a.map((y) => (y.id === x.id ? { ...y, on: !y.on } : y)));
  return (
    <>
      <FhIntro L={L} text={L.detIntro} addLabel={L.addDet} adding={adding} onAdd={() => setAdding(true)} />
      {adding && (
        <FhCard L={L} title={L.newDet} addLabel={exists && !errId ? L.updDet : L.addDet} canAdd={!!id} onCancel={close} onAdd={commit} error={tried ? errRules : null}>
          <FhField label={L.id} caption={L.detIdHint} info={info} error={tried ? errId : null}>
            <Input block mono ground="raised" placeholder="my-detector" value={d.id} invalid={!!(tried && errId)} onChange={set("id")} />
          </FhField>
          <FhField label={L.ext} caption={L.extHint}>
            <Input block mono ground="raised" placeholder="md, mdx" value={d.ext} invalid={!!(tried && errRules)} onChange={set("ext")} />
          </FhField>
          <FhField label={L.glob}>
            <Input block mono ground="raised" placeholder="Dockerfile" value={d.glob} invalid={!!(tried && errRules)} onChange={set("glob")} />
          </FhField>
        </FhCard>
      )}
      <div style={fhHead}>{L.regDet} ({xs.length})</div>
      <div style={{ display: "flex", flexDirection: "column" }}>
        {xs.length === 0 ? <FhEmpty>{L.emptyDet}</FhEmpty> : xs.map((x) => (
          <FhRow key={x.id} L={L} {...detRowProps(L, x)} onToggle={() => toggle(x)} onRemove={() => remove(x)} onUndo={() => undo(x)} />
        ))}
      </div>
    </>
  );
}

function FileHandlersPane({ lang = "en", empty = false, pending = false, tie = false, adding: adding0 = false, draft: draft0, attempted = false }) {
  const L = FH_COPY[lang] || FH_COPY.en;
  const [xs, setXs] = React.useState(() => {
    const base = empty ? [] : tie ? [...HDL_SEED, ...TIE_HDL] : HDL_SEED;
    return pending ? [...base.map((x) => (x.origin === "user" ? { ...x, pending: "remove" } : x)), ...PENDING_HDL] : base;
  });
  const blank = { short: "", det: "", prio: "", kind: "surface", surface: "", param: "", method: "" };
  const [adding, setAdding] = React.useState(adding0);
  const [d, setD] = React.useState({ ...blank, ...(draft0 || {}) });
  const [tried, setTried] = React.useState(attempted);
  const set = (k) => (e) => setD((o) => ({ ...o, [k]: e.target.value }));
  const short = d.short.trim(), full = "user/" + short;
  const detOpts = DET_SEED.map((x) => x.id);
  const exists = short ? xs.find((x) => x.id === full) : null;
  const errShort = !short ? null : !HDL_RE.test(short) ? L.errHdlChars : null;
  const errPrio = d.prio.trim() && !/^-?\d+$/.test(d.prio.trim()) ? L.errInt : null;
  const errMethod = d.kind === "ipc" && d.method.trim() && !/^[A-Za-z0-9_-]+(\.[A-Za-z0-9_-]+)+$/.test(d.method.trim()) ? L.errMethod : null;
  const reqKind = d.kind === "surface" ? !!d.surface.trim() : d.kind === "ipc" ? !!d.method.trim() : true;
  const canAdd = !!short && !!d.det && reqKind;
  const close = () => { setAdding(false); setD(blank); setTried(false); };
  const commit = () => {
    setTried(true);
    if (errShort || errPrio || errMethod) return;
    const act = d.kind === "surface" ? "surface:" + d.surface.trim() : d.kind === "ipc" ? "ipc:" + d.method.trim() : "system";
    const next = { id: full, prio: d.prio.trim() ? parseInt(d.prio, 10) : 100, origin: "user", det: d.det, act, on: true };
    setXs((a) => exists
      ? a.map((x) => (x.id !== full ? x : x.pending === "add" ? { ...next, pending: "add" } : { ...next, pending: "replace", prev: x.pending ? x.prev || strip(x) : x }))
      : [...a, { ...next, pending: "add" }]);
    close();
  };
  const remove = (x) => setXs((a) => a.map((y) => (y.id === x.id ? { ...y, pending: "remove" } : y)));
  const undo = (x) => setXs((a) => a.flatMap((y) => (y.id !== x.id ? [y] : y.pending === "add" ? [] : y.pending === "replace" ? [y.prev] : [strip(y)])));
  const toggle = (x) => setXs((a) => a.map((y) => (y.id === x.id ? { ...y, on: !y.on } : y)));
  const list = sortHdl(xs);
  return (
    <>
      <FhIntro L={L} text={L.hdlIntro} addLabel={L.addHdl} adding={adding} onAdd={() => setAdding(true)} />
      {adding && (
        <FhCard L={L} title={L.newHdl} addLabel={exists && !errShort ? L.updHdl : L.addHdl} canAdd={canAdd} onCancel={close} onAdd={commit}>
          <FhField label={L.short} caption={L.savedAs + " " + (short ? full : "user/…")} info={exists && !errShort ? L.infoReplaceHdl(full) : null} error={tried ? errShort : null}>
            <Input block mono ground="raised" placeholder="my-viewer" value={d.short} invalid={!!(tried && errShort)} onChange={set("short")} />
          </FhField>
          <FhField label={L.detector}>
            <span style={{ display: "flex", width: "var(--tasty-file-handler-select-width)", maxWidth: "100%" }}>
              <Select block ground="raised" placeholder={L.pickDet} options={detOpts} value={d.det} onChange={set("det")} />
            </span>
          </FhField>
          <FhField label={L.priority} caption={L.prioHint} error={tried ? errPrio : null}>
            <span style={{ display: "flex", width: "var(--tasty-file-handler-priority-width)" }}>
              <Input block mono ground="raised" placeholder="100" value={d.prio} invalid={!!(tried && errPrio)} onChange={set("prio")} />
            </span>
          </FhField>
          <FhField label={L.action} caption={d.kind === "system" ? L.systemHint : null}>
            <span style={{ display: "flex", width: "var(--tasty-file-handler-kind-width)", maxWidth: "100%" }}>
              <Select block ground="raised" options={[{ value: "surface", label: L.kinds.surface }, { value: "ipc", label: L.kinds.ipc }, { value: "system", label: L.kinds.system }]} value={d.kind} onChange={set("kind")} />
            </span>
          </FhField>
          {d.kind === "surface" && (
            <>
              <FhField label={L.surface}>
                <Input block mono ground="raised" placeholder="markdown_view" value={d.surface} onChange={set("surface")} />
              </FhField>
              <FhField label={L.param} caption={L.paramHint}>
                <Input block mono ground="raised" placeholder="file" value={d.param} onChange={set("param")} />
              </FhField>
            </>
          )}
          {d.kind === "ipc" && (
            <FhField label={L.method} error={tried ? errMethod : null}>
              <Input block mono ground="raised" placeholder="com.example.foo.open" value={d.method} invalid={!!(tried && errMethod)} onChange={set("method")} />
            </FhField>
          )}
        </FhCard>
      )}
      <div style={fhHead}>{L.regHdl} ({list.length})</div>
      <div style={{ display: "flex", flexDirection: "column" }}>
        {list.length === 0 ? <FhEmpty>{L.emptyHdl}</FhEmpty> : list.map((x) => {
          const rm = x.pending === "remove";
          return (
            <FhRow key={x.id} L={L} id={x.id} prio={x.prio} origins={[{ label: x.origin, plugin: x.origin.startsWith("plugin:") }]}
              pendingLabel={x.pending === "add" ? L.added : rm ? L.removed : x.pending === "replace" ? L.replaced : null}
              strikeId={rm} on={x.on && !rm} switchOn={x.on} switchDisabled={rm}
              trailing={x.origin !== "user" ? "lock" : x.pending ? "undo" : "remove"} lockedBy={x.origin}
              summary={hdlSummary(x)} summaryTitle={hdlSummary(x)} onToggle={() => toggle(x)} onRemove={() => remove(x)} onUndo={() => undo(x)} />
          );
        })}
      </div>
    </>
  );
}

export { FileDetectorsPane, FileHandlersPane };
