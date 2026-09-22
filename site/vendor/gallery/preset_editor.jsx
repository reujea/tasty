// Tasty Gallery — Preset editor (the PresetView window).
// Lives in its own file because the demo-layout PREVIEW is a real
// interactive component (not a static mock): mini tab strips switch,
// and in edit mode STRUCTURE is edited in place (edge-split / tab add+close /
// remove — WYSIWYG, auto-saved), while a leaf's PARAMETERS (kind · cwd ·
// startup · kind fields) open a SURFACE SETTINGS SCREEN that takes over the
// whole right detail column (header / scrolling form / fixed Cancel·OK footer)
// as a draft — the old in-cell LeafEditor clipped in small cells. Loaded BEFORE overlays-windows.jsx;
// exposes window.PresetEditor = { Section, NAV_ITEM }.
(function () {
  const { Section, Spec, Stage, Meta, Note, Do, Dont } = window.Gallery;
  const { Button, IconButton, Tag, Select, Input, Badge, Icon } = window.TastyDesignSystem_41fd3f;
  const { useState } = React;

  // ── icons — by name from the canonical set (icons/*.svg via <Icon/>) ──
  const I = {
    term: <Icon name="terminal" />,
    md: <Icon name="markdown" />,
    editor: <Icon name="edit" />,
    image: <Icon name="image" />,
    log: <Icon name="list" />,
    plus: <Icon name="plus" />,
    splitRow: <Icon name="split" />,
    splitCol: <Icon name="splitH" />,
    trash: <Icon name="trash" />,
    x: <Icon name="close" />,
    pencil: <Icon name="edit" />,
    copy: <Icon name="copy" />,
    layers: <Icon name="layers" />,
    check: <Icon name="check" />,
    gear: <Icon name="settings" />,
    folder: <Icon name="folderOpen" />,
    port: <Icon name="port" />,
    html: <Icon name="html" />,
  };

  // ── surface-kind registry (display_name + accent) ─────────
  const KINDS = {
    terminal: { label: "Terminal", icon: I.term, accent: "var(--tasty-accent-success)" },
    markdown: { label: "Markdown", icon: I.md, accent: "var(--tasty-accent-primary)" },
    editor: { label: "Editor", icon: I.editor, accent: "var(--tasty-accent-agent)" },
    log: { label: "Log", icon: I.log, accent: "var(--tasty-accent-warning)" },
    image: { label: "Image", icon: I.image, accent: "var(--tasty-accent-info)" },
    html: { label: "HTML", icon: I.html, accent: "var(--tasty-accent-attached)" },
    "plugin:portscan": { label: "Port scanner", icon: I.port, accent: "var(--tasty-accent-attention)" }, // a plugin kind — N fields, labels from the plugin
  };
  const KIND_KEYS = Object.keys(KINDS);
  const kindOf = (k) => KINDS[k] || KINDS.terminal;

  // ── leaf value-summary fields (mirrors KindCatalog::fields()) ──
  // [key, label, pathLike] — pathLike fields (cwd / file) truncate at the FRONT
  // (the tail — the leaf dir / filename — is what tells surfaces apart); command
  // / url fields truncate at the end. Unknown/plugin kinds fall back to cwd.
  const FIELDS = {
    terminal: [["cwd", "cwd", true], ["startup", "startup", false]],
    editor:   [["cwd", "cwd", true]],
    log:      [["cwd", "cwd", true]],
    markdown: [["file", "file", true]],
    image:    [["file", "file", true]],
    html:     [["url", "url", false]],
    "plugin:portscan": [["host", "host", false], ["range", "range", false]],
  };

  // ── settings-screen form fields (mirrors KindCatalog::form_fields()) ──
  // {key, label, type, placeholder?, options?, addon?} — type: dir | file (both get
  // a Browse button) | text | number | select. Labels come from the kind (plugin
  // kinds bring their own lang), so lengths vary — the form is one column, label
  // above input, so any label length fits. Kind is always the first control.
  const FORM_FIELDS = {
    terminal: [
      { key: "cwd", label: "Working directory", type: "dir", placeholder: "~" },
      { key: "startup", label: "Startup command", type: "text", placeholder: "(none)" },
    ],
    editor: [{ key: "cwd", label: "Working directory", type: "dir", placeholder: "~" }],
    log: [{ key: "cwd", label: "Working directory", type: "dir", placeholder: "~" }],
    markdown: [{ key: "file", label: "File", type: "file", placeholder: "README.md" }],
    image: [{ key: "file", label: "File", type: "file", placeholder: "image.png" }],
    html: [{ key: "url", label: "URL", type: "text", placeholder: "http://localhost:3000" }],
    "plugin:portscan": [
      { key: "host", label: "Host", type: "text", placeholder: "127.0.0.1" },
      { key: "range", label: "Port range", type: "text", placeholder: "1-65535" },
      { key: "protocol", label: "Protocol", type: "select", options: ["tcp", "udp", "tcp + udp"] },
      { key: "interval", label: "Scan interval", type: "number", addon: "ms", placeholder: "1000" },
      { key: "timeout", label: "Connect timeout", type: "number", addon: "ms", placeholder: "200" },
      { key: "output", label: "Output file", type: "file", placeholder: "ports.log" },
      { key: "notify", label: "Notify on change", type: "select", options: ["never", "new port", "any change"] },
    ],
  };
  const formFields = (kind) => FORM_FIELDS[kind] || [{ key: "cwd", label: "Working directory", type: "dir" }];
  // draft normalisation: only kind + the keys the kind declares take part in the diff.
  const normalize = (s) => {
    const o = { kind: s.kind };
    for (const f of formFields(s.kind)) o[f.key] = s[f.key] == null ? "" : String(s[f.key]);
    return o;
  };
  const isDirty = (draft, orig) => JSON.stringify(normalize(draft)) !== JSON.stringify(normalize(orig));
  // kind switch inside the draft: keep keys the new kind also declares (cwd survives terminal→editor), blank the rest.
  const switchKind = (draft, kind) => {
    const next = { kind };
    for (const f of formFields(kind)) next[f.key] = draft[f.key] != null ? draft[f.key] : "";
    return next;
  };

  // locate a leaf for the settings header: [pane N ›] tab › surface k
  function findNode(node, id) {
    if (node.id === id) return node;
    if (node.surface) return null;
    if (node.pane) { for (const t of node.pane.tabs) { const r = findNode(t.layout, id); if (r) return r; } return null; }
    return findNode(node.first, id) || findNode(node.second, id);
  }
  function leafIndex(layout, id) {
    let k = 0, found = null;
    const rec = (m) => { if (found) return; if (m.surface) { k++; if (m.id === id) found = k; return; } rec(m.first); rec(m.second); };
    rec(layout);
    return found;
  }
  function locate(root, id, scope) {
    if (scope === "tab") { const k = leafIndex(root, id); return k ? ["surface " + k] : []; }
    let paneNo = 0;
    const walk = (node) => {
      if (node.pane) {
        paneNo++;
        for (const t of node.pane.tabs) { const k = leafIndex(t.layout, id); if (k) return [...(scope === "workspace" ? ["pane " + paneNo] : []), t.name, "surface " + k]; }
        return null;
      }
      return walk(node.first) || walk(node.second);
    };
    return walk(root) || [];
  }

  // edit-mode context — SurfaceBox asks the host to open the settings screen.
  const EditCtx = React.createContext({ openCfg: null });
  // resolve the non-empty field rows for a leaf (empty fields hide their row).
  const summaryRows = (node) => {
    const s = node.surface;
    const defs = FIELDS[s.kind] || [["cwd", "cwd", true]];
    return defs
      .map(([key, label, pathLike]) => ({ label, value: s[key], pathLike }))
      .filter((r) => r.value != null && String(r.value).trim() !== "");
  };

  // ── tree builders ─────────────────────────────────────────
  let _uid = 0;
  const uid = (p) => p + (++_uid);
  const surf = (kind, x = {}) => ({ surface: { kind, cwd: "~/tasty", startup: "", ...x } });
  const split = (dir, ratio, a, b) => ({ dir, ratio, first: a, second: b });
  const tab = (name, layout) => ({ name, layout });
  const pane = (tabs, active = 0) => ({ pane: { tabs, active } });

  function idify(n) {
    if (n.surface) return { id: uid("s"), surface: { ...n.surface } };
    if (n.pane)
      return { id: uid("p"), pane: { active: n.pane.active || 0, tabs: n.pane.tabs.map((t) => ({ id: uid("t"), name: t.name, layout: idify(t.layout) })) } };
    return { id: uid("x"), dir: n.dir, ratio: n.ratio, first: idify(n.first), second: idify(n.second) };
  }

  // map a node (by id) to fn(node). Tabs are matched too (for active/rename).
  function mapNode(node, id, fn) {
    if (node.id === id) return fn(node);
    if (node.surface) return node;
    if (node.pane)
      return { ...node, pane: { ...node.pane, tabs: node.pane.tabs.map((t) => (t.id === id ? fn(t) : { ...t, layout: mapNode(t.layout, id, fn) })) } };
    return { ...node, first: mapNode(node.first, id, fn), second: mapNode(node.second, id, fn) };
  }
  // remove a surface/split node by id, collapsing its parent split into the sibling.
  function removeNode(node, id) {
    if (node.surface) return node;
    if (node.pane) return { ...node, pane: { ...node.pane, tabs: node.pane.tabs.map((t) => ({ ...t, layout: removeNode(t.layout, id) })) } };
    if (node.first.id === id) return node.second;
    if (node.second.id === id) return node.first;
    return { ...node, first: removeNode(node.first, id), second: removeNode(node.second, id) };
  }

  function applyAction(root, a) {
    switch (a.type) {
      case "kind":
        return mapNode(root, a.id, (n) => ({ ...n, surface: { ...n.surface, kind: a.kind } }));
      case "field":
        return mapNode(root, a.id, (n) => ({ ...n, surface: { ...n.surface, [a.key]: a.value } }));
      case "surface": // settings-screen OK: the whole draft lands at once, then auto-saves
        return mapNode(root, a.id, (n) => ({ ...n, surface: { ...a.surface } }));
      case "split":
        return mapNode(root, a.id, (n) => a.before
          ? { id: uid("x"), dir: a.dir, ratio: 0.5, first: idify(surf("terminal")), second: n }
          : { id: uid("x"), dir: a.dir, ratio: 0.5, first: n, second: idify(surf("terminal")) });
      case "del":
        if (root.id === a.id) return root; // can't delete the sole surface
        return removeNode(root, a.id);
      case "closeTab":
        return mapNode(root, a.paneId, (n) => {
          if (n.pane.tabs.length <= 1) return n; // guard: never close the last tab
          const tabs = n.pane.tabs.filter((_, i) => i !== a.idx);
          let active = n.pane.active;
          if (a.idx < active) active -= 1;
          active = Math.max(0, Math.min(active, tabs.length - 1));
          return { ...n, pane: { tabs, active } };
        });
      case "active":
        return mapNode(root, a.paneId, (n) => ({ ...n, pane: { ...n.pane, active: a.idx } }));
      case "addTab":
        return mapNode(root, a.paneId, (n) => ({
          ...n,
          pane: { tabs: [...n.pane.tabs, { id: uid("t"), name: "shell", layout: idify(surf("terminal")) }], active: n.pane.tabs.length },
        }));
      default:
        return root;
    }
  }

  // ── leaf surface box ──────────────────────────────────────
  // Edit mode = direct manipulation (mirrors the real tasty pane): hover an
  // edge (~30% band) to preview a split in that direction, click to commit;
  // click the CENTER to select the leaf (2px accent outline + settings/remove
  // handles); the settings handle or a DOUBLE-CLICK opens the surface settings
  // screen in the detail column. Selection stays the target of the structure
  // shortcuts (split / remove / new tab) — that is why a single click does NOT
  // open the screen (decision A1).
  const SPLIT_ZONE_EDGE = 0.3;   // outer 30% of each side is a split band
  const SPLIT_ZONE_MIN = 46;     // below this px on an axis, that axis degrades (no band)
  function SurfaceBox({ node, edit, sel, dispatch, setSel }) {
    const k = kindOf(node.surface.kind);
    const selected = edit && sel === node.id;
    const { openCfg } = React.useContext(EditCtx);
    const ref = React.useRef(null);
    const [zone, setZone] = useState(null); // 'left'|'right'|'top'|'bottom'|null
    const [box, setBox] = useState({ w: 0, h: 0 });
    React.useEffect(() => {
      const el = ref.current;
      if (!el || typeof ResizeObserver === "undefined") return;
      const ro = new ResizeObserver(() => {
        setBox({ w: el.offsetWidth, h: el.offsetHeight }); // border-box
      });
      ro.observe(el);
      return () => ro.disconnect();
    }, []);
    // degrade order: full → (w<96 or h<72) drop summary → (short axis <46) drop kind label, icon only.
    const shortAxis = Math.min(box.w, box.h);
    const showLabel = shortAxis === 0 || shortAxis >= SPLIT_ZONE_MIN;
    const showSummary = box.w >= 96 && box.h >= 72;
    const rows = showSummary ? summaryRows(node) : [];

    const pickZone = (e) => {
      if (!edit || selected || !ref.current) return setZone(null);
      const r = ref.current.getBoundingClientRect();
      const x = (e.clientX - r.left) / r.width, y = (e.clientY - r.top) / r.height;
      const cands = [];
      if (r.width >= SPLIT_ZONE_MIN) cands.push(["left", x], ["right", 1 - x]);
      if (r.height >= SPLIT_ZONE_MIN) cands.push(["top", y], ["bottom", 1 - y]);
      let best = null, bestD = SPLIT_ZONE_EDGE;
      for (const [name, d] of cands) if (d < bestD) { bestD = d; best = name; }
      setZone(best);
    };
    const onClick = (e) => {
      if (!edit) return;
      e.stopPropagation();
      if (zone) {
        const dir = zone === "left" || zone === "right" ? "row" : "col";
        const before = zone === "left" || zone === "top";
        dispatch({ type: "split", id: node.id, dir, before });
        setZone(null);
      } else {
        setSel(node.id);
      }
    };
    const bandStyle = {
      left:   { left: 0, top: 0, bottom: 0, width: "30%", borderRight: "2px solid var(--tasty-preset-split-zone-border)" },
      right:  { right: 0, top: 0, bottom: 0, width: "30%", borderLeft: "2px solid var(--tasty-preset-split-zone-border)" },
      top:    { left: 0, right: 0, top: 0, height: "30%", borderBottom: "2px solid var(--tasty-preset-split-zone-border)" },
      bottom: { left: 0, right: 0, bottom: 0, height: "30%", borderTop: "2px solid var(--tasty-preset-split-zone-border)" },
    };

    return (
      <div
        ref={ref}
        onClick={edit ? onClick : undefined}
        onDoubleClick={edit && openCfg ? (e) => { e.stopPropagation(); setSel(node.id); openCfg(node.id); } : undefined}
        onMouseMove={edit && !selected ? pickZone : undefined}
        onMouseLeave={edit ? () => setZone(null) : undefined}
        style={{
          position: "relative", flex: 1, minWidth: 0, minHeight: 0, display: "flex", flexDirection: "column",
          alignItems: "center", justifyContent: "center", gap: 6, padding: 6, overflow: "hidden",
          background: "var(--tasty-bg-app)", cursor: edit ? (zone ? "crosshair" : "pointer") : "default",
          boxShadow: selected
            ? "inset 0 0 0 2px var(--tasty-accent-primary)"
            : edit ? "inset 0 0 0 1px var(--tasty-separator)" : "none",
        }}
      >
        <span style={{ display: "inline-flex", color: k.accent }}>{k.icon}</span>
        {showLabel && (
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-secondary)", whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden", maxWidth: "100%" }}>{k.label}</span>
        )}
        {rows.length > 0 && <LeafSummary rows={rows} />}
        {/* split-zone preview overlay (hover, unselected) */}
        {zone && !selected && (
          <div style={{ position: "absolute", background: "var(--tasty-preset-split-zone-bg)", pointerEvents: "none", zIndex: 2, ...bandStyle[zone] }} />
        )}
        {/* selected → settings + remove handles (splitting is done by the edge zones) */}
        {selected && (
          <div style={{ position: "absolute", top: 4, right: 4, display: "flex", gap: 2, zIndex: 3 }}>
            {openCfg && <MiniHandle title="Surface settings" onClick={(e) => { e.stopPropagation(); openCfg(node.id); }}>{I.gear}</MiniHandle>}
            <MiniHandle title="Remove" danger onClick={(e) => { e.stopPropagation(); dispatch({ type: "del", id: node.id }); setSel(null); }}>{I.trash}</MiniHandle>
          </div>
        )}
      </div>
    );
  }

  function MiniHandle({ children, onClick, title, danger }) {
    return (
      <button title={title} onClick={onClick} style={{
        appearance: "none", cursor: "pointer", width: 18, height: 18, display: "inline-flex", alignItems: "center", justifyContent: "center",
        padding: 0, borderRadius: "var(--tasty-radius-sm)", border: "1px solid var(--tasty-border-strong)",
        background: "var(--tasty-surface-raised)", color: danger ? "var(--tasty-accent-danger)" : "var(--tasty-text-secondary)",
      }}>
        <span style={{ display: "inline-flex", transform: "scale(.62)" }}>{children}</span>
      </button>
    );
  }

  // ── leaf value summary (unselected leaf, read-only + edit mode) ──
  // Below the kind icon + name: one `label value` row per configured field.
  // Labels muted-micro, values secondary-caption, both mono. Path-like fields
  // truncate at the front (keep the tail); commands/urls truncate at the end.
  function LeafSummary({ rows }) {
    return (
      <div style={{ width: "100%", maxWidth: 220, minWidth: 0, display: "flex", flexDirection: "column",
        gap: "var(--tasty-preset-leaf-summary-gap)", padding: "0 8px" }}>
        {rows.map((r) => (
          <div key={r.label} style={{ display: "flex", gap: 6, minWidth: 0, alignItems: "baseline", justifyContent: "center" }}>
            <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-preset-leaf-label-font-size)", color: "var(--tasty-preset-leaf-label-fg)" }}>{r.label}</span>
            <span style={{ minWidth: 0, flex: "0 1 auto", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-preset-leaf-value-font-size)", color: "var(--tasty-preset-leaf-value-fg)",
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              ...(r.pathLike ? { direction: "rtl", unicodeBidi: "plaintext", textAlign: "left" } : {}) }}>{r.value}</span>
          </div>
        ))}
      </div>
    );
  }

  // ── SURFACE SETTINGS SCREEN — replaces the detail column (toolbar + preview) ──
  // Three boxes: header (kind icon + name · location breadcrumb · draft state),
  // scrolling body (one-column form, capped at form-max-width), fixed footer
  // (Cancel ghost · OK primary, right-aligned — Settings-footer grammar).
  // Keys: Esc = Cancel; Enter inside a single-line input = OK (when dirty).
  // Fixed dialog keys read directly in code (like convert / command palette) —
  // NOT KeybindingSettings entries. Structure shortcuts
  // (split / close / new tab) are inert while this is open — the preview is hidden.
  function SurfaceSettings({ draft, orig, path = [], onChange, onConfirm, onCancel, bodyRef, autoFocus = false }) {
    const k = kindOf(draft.kind);
    const dirty = isDirty(draft, orig);
    const fields = formFields(draft.kind);
    const set = (key, value) => onChange({ ...draft, [key]: value });
    const onKeyDown = (e) => {
      if (e.key === "Escape") { e.preventDefault(); onCancel(); }
      else if (e.key === "Enter" && e.target.tagName === "INPUT" && dirty) { e.preventDefault(); onConfirm(); }
    };
    return (
      <div onKeyDown={onKeyDown} onClick={(e) => e.stopPropagation()}
        style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)" }}>
        {/* header */}
        <div style={{ display: "flex", alignItems: "center", gap: 8, height: "var(--tasty-preset-cfg-header-height)", flex: "none", padding: "0 12px", minWidth: 0, borderBottom: "1px solid var(--tasty-separator)" }}>
          <span style={{ display: "inline-flex", flex: "none", color: k.accent }}>{k.icon}</span>
          <span style={{ fontSize: 14, fontWeight: 600, flex: "none" }}>{k.label}</span>
          {path.length > 0 && (
            <span style={{ display: "flex", alignItems: "center", gap: 5, minWidth: 0, overflow: "hidden", fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", whiteSpace: "nowrap" }}>
              {path.map((seg, i) => (
                <React.Fragment key={i}>
                  {i > 0 && <span style={{ flex: "none" }}>›</span>}
                  <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>{seg}</span>
                </React.Fragment>
              ))}
            </span>
          )}
          <div style={{ flex: 1 }} />
          {dirty && (
            <span title="Draft — applied on OK" style={{ display: "inline-flex", alignItems: "center", gap: 5, flex: "none", fontSize: 11, color: "var(--tasty-text-muted)" }}>
              <span style={{ width: 6, height: 6, borderRadius: "var(--tasty-radius-full)", background: "var(--tasty-preset-cfg-draft-fg)" }} />unsaved
            </span>
          )}
        </div>
        {/* body — the only box that scrolls */}
        <div ref={bodyRef} className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: "var(--tasty-preset-cfg-form-padding)" }}>
          <div style={{ maxWidth: "var(--tasty-preset-cfg-form-max-width)", display: "flex", flexDirection: "column", gap: "var(--tasty-preset-cfg-field-gap)" }}>
            <Field label="Kind">
              <Select block autoFocus={autoFocus} value={draft.kind} onChange={(e) => onChange(switchKind(draft, e.target.value))}
                options={KIND_KEYS.map((kk) => ({ value: kk, label: kindOf(kk).label }))} />
            </Field>
            {fields.map((f) => (
              <Field key={draft.kind + ":" + f.key} label={f.label}>
                {f.type === "select" ? (
                  <Select block value={draft[f.key] || f.options[0]} onChange={(e) => set(f.key, e.target.value)} options={f.options} />
                ) : f.type === "dir" || f.type === "file" ? (
                  <div style={{ display: "flex", gap: 8, minWidth: 0 }}>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <Input block mono placeholder={f.placeholder} value={draft[f.key] || ""} onChange={(e) => set(f.key, e.target.value)} />
                    </div>
                    <Button variant="secondary" leadingIcon={I.folder} style={{ flex: "none" }}>Browse</Button>
                  </div>
                ) : (
                  <Input block mono type={f.type === "number" ? "number" : "text"} addon={f.addon} placeholder={f.placeholder} value={draft[f.key] || ""} onChange={(e) => set(f.key, e.target.value)} />
                )}
              </Field>
            ))}
          </div>
        </div>
        {/* fixed footer */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 8, flex: "none", height: "var(--tasty-preset-cfg-footer-height)", padding: "0 var(--tasty-preset-cfg-footer-padding-x)", borderTop: "1px solid var(--tasty-separator)" }}>
          <Button variant="ghost" onClick={onCancel}>Cancel</Button>
          <Button variant="primary" disabled={!dirty} onClick={onConfirm}>OK</Button>
        </div>
      </div>
    );
  }
  // host-side draft state for the settings screen: { id, draft, orig } | null
  function useSurfaceCfg(root, setRoot, setSel) {
    const [cfg, setCfg] = useState(null);
    const openCfg = (id) => { const n = findNode(root, id); if (n) setCfg({ id, draft: { ...n.surface }, orig: { ...n.surface } }); };
    const cancel = () => setCfg(null);                         // draft dropped, selection kept
    const confirm = () => { if (!cfg) return; setRoot((r) => applyAction(r, { type: "surface", id: cfg.id, surface: normalize(cfg.draft) })); setSel(cfg.id); setCfg(null); };
    const change = (draft) => setCfg((c) => (c ? { ...c, draft } : c));
    return { cfg, openCfg, cancel, confirm, change, reset: () => setCfg(null) };
  }
  function Field({ label, children }) {
    return (
      <label style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>{label}</span>
        {children}
      </label>
    );
  }

  // ── surface tree (the LOWER / surface-split layout, inside a tab) ──
  function SurfaceView({ node, edit, sel, dispatch, setSel }) {
    if (node.surface) return <SurfaceBox node={node} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />;
    const row = node.dir === "row";
    const child = (n, grow) => (
      <div style={{ flexGrow: grow, flexBasis: 0, minWidth: 0, minHeight: 0, display: "flex" }}>
        <SurfaceView node={n} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />
      </div>
    );
    return (
      <div style={{ display: "flex", flexDirection: row ? "row" : "column", flex: 1, minWidth: 0, minHeight: 0 }}>
        {child(node.first, node.ratio)}
        {/* surface-split divider = thin hairline */}
        <div title="Surface split" style={{ flex: "none", background: "var(--tasty-border-default)", ...(row ? { width: 1 } : { height: 1 }) }} />
        {child(node.second, 1 - node.ratio)}
      </div>
    );
  }

  // ── one pane = mini tab strip + active tab's surface layout ──
  function Pane({ node, edit, sel, dispatch, setSel }) {
    const p = node.pane;
    const active = p.tabs[p.active] || p.tabs[0];
    const [hoverTab, setHoverTab] = useState(null);
    const canClose = edit && p.tabs.length > 1;
    return (
      <div style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", flexDirection: "column",
        border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", background: "var(--tasty-bg-app)" }}>
        {/* mini tab strip */}
        <div style={{ display: "flex", alignItems: "stretch", flex: "none", minWidth: 0, overflow: "hidden", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
          {p.tabs.map((t, i) => {
            const on = i === p.active;
            const showX = canClose && (on || hoverTab === i);
            return (
              <button key={t.id} onClick={(e) => { e.stopPropagation(); dispatch({ type: "active", paneId: node.id, idx: i }); }}
                onMouseEnter={edit ? () => setHoverTab(i) : undefined} onMouseLeave={edit ? () => setHoverTab(null) : undefined}
                style={{ appearance: "none", cursor: "pointer", display: "inline-flex", alignItems: "center", gap: 5, height: 20, padding: showX ? "0 3px 0 9px" : "0 9px",
                  border: 0, borderRight: "1px solid var(--tasty-separator)", flex: "0 1 auto", minWidth: 0,
                  fontFamily: "var(--tasty-font-ui)", fontSize: 11,
                  background: on ? "var(--tasty-bg-panel)" : "transparent",
                  color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
                  boxShadow: on ? "inset 0 -2px 0 var(--tasty-accent-primary)" : "none" }}>
                <span style={{ display: "inline-flex", flex: "none", color: on ? kindOf(activeKind(t)).accent : "var(--tasty-text-muted)", transform: "scale(.8)" }}>{kindOf(activeKind(t)).icon}</span>
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 }}>{t.name}</span>
                {showX && (
                  <span role="button" aria-label={"Close " + t.name} title="Close tab"
                    onClick={(e) => { e.stopPropagation(); dispatch({ type: "closeTab", paneId: node.id, idx: i }); if (hoverTab === i) setHoverTab(null); }}
                    onMouseDown={(e) => e.stopPropagation()}
                    style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", flex: "none", width: 14, height: 14, marginLeft: 1,
                      borderRadius: "var(--tasty-radius-sm)", color: "var(--tasty-text-muted)", cursor: "pointer" }}
                    onMouseEnter={(e) => { e.currentTarget.style.background = "var(--tasty-overlay-active)"; e.currentTarget.style.color = "var(--tasty-text-primary)"; }}
                    onMouseOut={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--tasty-text-muted)"; }}>
                    <span style={{ display: "inline-flex", transform: "scale(.5)" }}>{I.x}</span>
                  </span>
                )}
              </button>
            );
          })}
          {edit && <AddTabBtn onClick={(e) => { e.stopPropagation(); dispatch({ type: "addTab", paneId: node.id }); }} />}
        </div>
        {/* active tab body */}
        <div style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", padding: 3, background: "var(--tasty-bg-app)" }}>
          <SurfaceView node={active.layout} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />
        </div>
      </div>
    );
  }
  // add-tab "+" at the strip end — mirrors the real TabBar's trailing add button.
  // Muted at rest, secondary + hover fill on hover (matches IconButton hover).
  function AddTabBtn({ onClick }) {
    const [hover, setHover] = useState(false);
    return (
      <button title="Add tab" aria-label="Add tab" onClick={onClick}
        onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
        style={{ appearance: "none", cursor: "pointer", display: "inline-flex", alignItems: "center", justifyContent: "center", flex: "none",
          width: 22, height: 20, border: 0, background: hover ? "var(--tasty-overlay-hover)" : "transparent",
          color: hover ? "var(--tasty-text-secondary)" : "var(--tasty-text-muted)" }}>
        <span style={{ display: "inline-flex", transform: "scale(.7)" }}>{I.plus}</span>
      </button>
    );
  }
  // representative kind of a tab (first leaf) — drives the mini-tab icon
  function activeKind(t) {
    let n = t.layout;
    while (!n.surface) n = n.first;
    return n.surface.kind;
  }

  // ── pane tree (the UPPER / pane-split layout) ─────────────
  function PaneTree({ node, edit, sel, dispatch, setSel }) {
    if (node.pane) return <Pane node={node} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />;
    const row = node.dir === "row";
    const child = (n, grow) => (
      <div style={{ flexGrow: grow, flexBasis: 0, minWidth: 0, minHeight: 0, display: "flex" }}>
        <PaneTree node={n} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />
      </div>
    );
    return (
      <div style={{ display: "flex", flexDirection: row ? "row" : "column", flex: 1, minWidth: 0, minHeight: 0, gap: 5, background: "var(--tasty-bg-app)" }}>
        {child(node.first, node.ratio)}
        {child(node.second, 1 - node.ratio)}
      </div>
    );
    // the 5px bg-app gap between bordered pane cards IS the pane (upper) divider — heavier than a surface hairline.
  }

  // ── scope-aware preview body ──────────────────────────────
  function PreviewBody({ scope, root, edit, sel, dispatch, setSel }) {
    if (scope === "tab") {
      // single tab → just its surface layout, framed like a tab body
      return (
        <div style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", padding: 3, background: "var(--tasty-bg-app)" }}>
          <SurfaceView node={root} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />
        </div>
      );
    }
    // workspace (pane tree) or pane (single pane)
    return <PaneTree node={root} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />;
  }

  // ── standalone interactive preview (used by the 3-scope + WYSIWYG specs) ──
  // In edit mode the settings handle / double-click swaps the frame for the
  // surface settings screen, exactly like the window's detail column.
  function LivePreview({ scope, build, edit = false, height = 230 }) {
    const [root, setRoot] = useState(() => idify(build()));
    const [sel, setSel] = useState(null);
    const dispatch = (a) => setRoot((r) => applyAction(r, a));
    const c = useSurfaceCfg(root, setRoot, setSel);
    const ctx = React.useMemo(() => ({ openCfg: edit ? c.openCfg : null }), [edit, root]);
    return (
      <EditCtx.Provider value={ctx}>
        <div onClick={edit ? () => setSel(null) : undefined}
          style={{ height, display: "flex", padding: c.cfg ? 0 : 10, background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)", border: "1px solid var(--tasty-border-default)", overflow: "hidden" }}>
          {c.cfg
            ? <SurfaceSettings draft={c.cfg.draft} orig={c.cfg.orig} path={locate(root, c.cfg.id, scope)} onChange={c.change} onConfirm={c.confirm} onCancel={c.cancel} autoFocus />
            : <PreviewBody scope={scope} root={root} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />}
        </div>
      </EditCtx.Provider>
    );
  }

  // ── demo presets ──────────────────────────────────────────
  const buildWorkspace = () =>
    split("row", 0.6,
      pane([
        tab("edit", split("col", 0.64, surf("editor", { cwd: "~/tasty/src" }), surf("terminal", { startup: "cargo watch" }))),
        tab("agent", surf("editor", { cwd: "~/tasty" })),
      ], 0),
      pane([
        tab("preview", surf("markdown", { cwd: "~/tasty/docs", file: "docs/architecture.md" })),
        tab("logs", split("row", 0.5, surf("log"), surf("terminal", { startup: "tail -f" }))),
      ], 0),
    );
  const buildTab = () =>
    split("row", 0.5, surf("editor", { cwd: "~/tasty/src" }), split("col", 0.5, surf("terminal", { startup: "cargo build" }), surf("log")));
  const buildPane = () =>
    pane([
      tab("server", surf("terminal", { startup: "npm start" })),
      tab("dev", split("col", 0.5, surf("terminal", { startup: "vite" }), surf("log"))),
      tab("notes", surf("markdown", { file: "NOTES.md" })),
    ], 0);

  // for the WYSIWYG spec: a roomy leaf so the handles + summary read at a glance
  const buildEditDemo = () =>
    pane([
      tab("dev", split("row", 0.46, surf("terminal", { cwd: "~/tasty", startup: "cargo watch -x run" }), surf("log"))),
      tab("docs", surf("markdown", { file: "README.md" })),
    ], 0);
  // the six-surface workspace from the bug report — cells too small for any in-cell form
  const buildSix = () =>
    split("row", 0.5,
      pane([tab("code", split("col", 0.5, surf("editor", { cwd: "~/tasty/src" }), split("row", 0.5, surf("terminal", { startup: "cargo watch" }), surf("terminal", { startup: "cargo test" }))))]),
      pane([tab("ops", split("col", 0.5, split("row", 0.5, surf("log"), surf("plugin:portscan", { host: "127.0.0.1", range: "3000-3999", protocol: "tcp", interval: "1000", timeout: "200", output: "", notify: "new port" })), surf("markdown", { file: "docs/runbook.md" })))]),
    );

  // ── the full window (list + toolbar + preview, with Edit toggle) ──
  const PRESETS = {
    workspace: [
      { name: "claude", subtitle: "editor · agent · logs", build: buildWorkspace },
      { name: "six", subtitle: "6 surfaces · code + ops", build: buildSix },
      { name: "dev", subtitle: "2 panes · shell + watch", build: () => split("row", 0.5, pane([tab("shell", surf("terminal"))]), pane([tab("logs", surf("log"))])) },
      { name: "review", subtitle: "diff + terminal", build: () => split("col", 0.55, pane([tab("diff", surf("editor"))]), pane([tab("run", surf("terminal"))])) },
    ],
    tab: [
      { name: "split-shell", subtitle: "editor + run + log", build: buildTab },
      { name: "single", subtitle: "1 surface · terminal", build: () => surf("terminal") },
    ],
    pane: [
      { name: "triple", subtitle: "3 tabs · server/dev/notes", build: buildPane },
      { name: "watch", subtitle: "2 tabs · build + test", build: () => pane([tab("build", surf("terminal")), tab("test", surf("terminal"))]) },
    ],
  };
  const SCOPES = [["workspace", "Workspace"], ["tab", "Tab"], ["pane", "Pane"]];

  function PresetWindow() {
    const [scope, setScope] = useState("workspace");
    const [pick, setPick] = useState(0);
    const [edit, setEdit] = useState(false);
    const list = PRESETS[scope];
    const cur = list[Math.min(pick, list.length - 1)];

    // live tree + selection, re-seeded when the chosen preset changes
    const seedKey = scope + ":" + pick;
    const [root, setRoot] = useState(() => idify(cur.build()));
    const [sel, setSel] = useState(null);
    const [seed, setSeed] = useState(seedKey);
    const dispatch = (a) => setRoot((r) => applyAction(r, a));
    const c = useSurfaceCfg(root, setRoot, setSel);
    const cfgOpen = !!c.cfg;
    const ctx = React.useMemo(() => ({ openCfg: edit ? c.openCfg : null }), [edit, root]);
    // an outside selection change (context-menu "save as preset…", window close) drops the draft like Cancel
    if (seed !== seedKey) { setSeed(seedKey); setRoot(idify(cur.build())); setSel(null); setEdit(false); c.reset(); }

    const choose = (i) => { setPick(i); };
    // left list + L1 scope are inert while the settings screen is open (decision C)
    const dim = cfgOpen ? { opacity: "var(--tasty-preset-cfg-dim-opacity)", pointerEvents: "none" } : {};

    return (
      <EditCtx.Provider value={ctx}>
      <div style={{ width: "100%", maxWidth: 680, minWidth: 0, height: 452, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
        border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
        {/* title bar */}
        <div style={{ display: "flex", alignItems: "center", gap: 8, height: 36, flex: "none", padding: "0 12px", borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{I.layers}</span>
          <span style={{ fontSize: 14, fontWeight: 600 }}>Layout presets</span>
          <div style={{ flex: 1 }} />
          <IconButton size="sm" aria-label="Close">{I.x}</IconButton>
        </div>
        {/* L1 scope tabs */}
        <div aria-disabled={cfgOpen || undefined} style={{ display: "flex", alignItems: "center", height: 40, flex: "none", padding: "0 12px", gap: 2, borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)", ...dim }}>
          {SCOPES.map(([id, lbl]) => {
            const on = id === scope;
            return (
              <button key={id} onClick={() => { setScope(id); setPick(0); }}
                style={{ appearance: "none", cursor: "pointer", height: 39, padding: "0 13px", border: 0, background: "transparent", fontFamily: "var(--tasty-font-ui)", fontSize: 13,
                  color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: on ? 600 : 400,
                  borderBottom: on ? "2px solid var(--tasty-accent-primary)" : "2px solid transparent" }}>{lbl}</button>
            );
          })}
        </div>
        {/* body: list + detail */}
        <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
          {/* left list */}
          <div aria-disabled={cfgOpen || undefined} style={{ width: 196, flex: "none", display: "flex", flexDirection: "column", background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)", ...dim }}>
            <div style={{ display: "flex", alignItems: "center", padding: "8px 10px 4px" }}>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>{list.length} preset{list.length === 1 ? "" : "s"}</span>
              <div style={{ flex: 1 }} />
              <IconButton size="sm" aria-label="New preset">{I.plus}</IconButton>
            </div>
            <div style={{ flex: 1, overflow: "auto", padding: "0 6px 6px" }}>
              {list.map((p, i) => {
                const on = i === Math.min(pick, list.length - 1);
                return (
                  <button key={p.name} onClick={() => choose(i)}
                    style={{ appearance: "none", cursor: "pointer", textAlign: "left", width: "100%", display: "flex", flexDirection: "column", gap: 1,
                      padding: "7px 9px", border: 0, marginTop: 1, borderRadius: "var(--tasty-radius-sm)",
                      background: on ? "var(--tasty-surface-active)" : "transparent",
                      boxShadow: on ? "inset 2px 0 0 var(--tasty-accent-primary)" : "none" }}>
                    <span style={{ fontSize: 13, color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)" }}>{p.name}</span>
                    <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10.5, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.subtitle}</span>
                  </button>
                );
              })}
            </div>
          </div>
          {/* right: surface settings screen (draft) OR toolbar + preview */}
          {cfgOpen ? (
            <SurfaceSettings draft={c.cfg.draft} orig={c.cfg.orig} path={[cur.name, ...locate(root, c.cfg.id, scope)]} onChange={c.change} onConfirm={c.confirm} onCancel={c.cancel} autoFocus />
          ) : (
          <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)" }}>
            {/* toolbar */}
            <div style={{ display: "flex", alignItems: "center", gap: 8, height: 44, flex: "none", padding: "0 12px", borderBottom: "1px solid var(--tasty-separator)" }}>
              {edit ? (
                <input defaultValue={cur.name} aria-label="Preset name"
                  style={{ width: 150, height: 26, border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)", color: "var(--tasty-text-primary)", font: "inherit", fontSize: 13, padding: "0 8px" }} />
              ) : (
                <>
                  <span style={{ fontSize: 14, fontWeight: 600 }}>{cur.name}</span>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{cur.subtitle}</span>
                </>
              )}
              <div style={{ flex: 1 }} />
              {edit ? (
                <>
                  <span style={{ display: "inline-flex", alignItems: "center", gap: 5, fontSize: 11, color: "var(--tasty-text-muted)", marginRight: 4 }}>
                    <span style={{ display: "inline-flex", color: "var(--tasty-accent-success)", transform: "scale(.8)" }}>{I.check}</span> saved automatically
                  </span>
                  <Button size="sm" variant="primary" leadingIcon={I.check} onClick={() => { setEdit(false); setSel(null); }}>Done</Button>
                </>
              ) : (
                <>
                  <IconButton size="sm" aria-label="Rename">{I.pencil}</IconButton>
                  <IconButton size="sm" aria-label="Duplicate">{I.copy}</IconButton>
                  <IconButton size="sm" aria-label="Delete">{I.trash}</IconButton>
                  <span style={{ width: 1, height: 18, background: "var(--tasty-separator)", margin: "0 4px" }} />
                  <Button size="sm" variant="secondary" leadingIcon={I.pencil} onClick={() => setEdit(true)}>Edit</Button>
                </>
              )}
            </div>
            {/* preview region */}
            <div onClick={edit ? () => setSel(null) : undefined} style={{ flex: 1, minHeight: 0, display: "flex", padding: 12, background: "var(--tasty-bg-app)" }}>
              <PreviewBody scope={scope} root={root} edit={edit} sel={sel} dispatch={dispatch} setSel={setSel} />
            </div>
          </div>
          )}
        </div>
      </div>
      </EditCtx.Provider>
    );
  }

  // ── static state frames for the settings-screen spec ────────────
  function SettingsDemo({ label, surface, orig, path, width = 300, height = 300, scrollTo = 0 }) {
    const [draft, setDraft] = useState(surface);
    const [base, setBase] = useState(orig || surface);
    const bodyRef = React.useRef(null);
    React.useEffect(() => { if (bodyRef.current && scrollTo) bodyRef.current.scrollTop = scrollTo; }, []);
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 0 }}>
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
        <div style={{ width, height, display: "flex", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
          <SurfaceSettings draft={draft} orig={base} path={path} bodyRef={bodyRef} onChange={setDraft}
            onConfirm={() => setBase(draft)} onCancel={() => setDraft(base)} />
        </div>
      </div>
    );
  }

  // ── the section ───────────────────────────────────────────
  function PresetEditorSection() {
    return (
      <Section id="preseteditor" title="Preset editor window">
        <Spec title="PresetView — list → toolbar + live preview"
          when={<>The modeless window behind <b>Tools › Presets</b>. <b>L1 tabs</b> pick the scope (Workspace / Tab / Pane); a <b>left list</b> of saved presets for that scope feeds a <b>right detail</b> = a toolbar (rename · duplicate · delete · <b>Edit</b>) over a <b>live demo-layout preview</b>. The selected row uses the surface-fill + 2px accent left-bar (the file-handler / sidebar idiom). Click a preset, switch its mini tabs, then hit <b>Edit</b> — structure edits happen on the preview, a leaf's gear opens its settings screen in this same column.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><PresetWindow /></Stage>
          <Meta
            specs={[["frame", "up to 680 × 452 (adaptive)"], ["L1 tabs", "40px, accent underline"], ["list", "196px, fill + 2px bar"], ["toolbar", "44px, Edit on the right"], ["preview", "flex, on --tasty-bg-app"]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "L1 + list", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-bg-panel", use: "detail", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-surface-active", use: "selected preset", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-primary", use: "active tab / select bar", color: "var(--tasty-accent-primary)" }]} />
          <Note>This is the <b>2-depth idiom</b> (L1 fixed scope tabs → growable L2 list → detail), reused from Settings — not a new shell. The detail column has two fillings: the demo-layout preview (below) and, while a surface is being configured, the <b>surface settings screen</b> (last spec). Try <b>six</b> › Edit › select a small cell › gear.</Note>
        </Spec>

        <Spec title="Demo-layout preview — the new component"
          when={<>An <b>interactive miniature</b> of a preset's tree — not a screenshot. It draws the three structural levels with distinct visual weights: <b>pane split</b> (upper layout) as separate bordered cards with a wide app-bg gap; <b>tab strip</b> as a mini 20px row whose active tab carries the accent bar; <b>surface split</b> (lower layout) as a thin hairline; and each <b>surface leaf</b> as its <b>kind</b> plus a <b>summary of its configured fields</b> (<code>cwd</code> / <code>startup</code> / <code>file</code> / <code>url</code>) so same-kind leaves are told apart. Mini tabs are live — click one to swap that pane's surface layout.</>}>
          <Stage variant="tight" grid style={{ alignItems: "stretch" }}>
            <div style={{ display: "grid", gridTemplateColumns: "1.5fr 1fr 1fr", gap: 14, width: "100%" }}>
              <ScopeDemo label="Workspace" sub="pane split + tabs + surface split" scope="workspace" build={buildWorkspace} />
              <ScopeDemo label="Tab" sub="surface split tree only" scope="tab" build={buildTab} />
              <ScopeDemo label="Pane" sub="tab strip + active tab" scope="pane" build={buildPane} />
            </div>
          </Stage>
          <Meta
            specs={[["pane split", "bordered cards · 5px app-bg gap"], ["tab strip", "20px mini row · accent bar"], ["surface split", "1px hairline (lower layout)"], ["leaf", "icon + kind + field summary"], ["summary", "label:value rows · mono · front-trunc paths"], ["degrade", "drop summary <96×72 · icon-only <46"]]}
            tokens={[{ tok: "--tasty-bg-app", use: "surface fill / pane gap", color: "var(--tasty-bg-app)" }, { tok: "--tasty-border-default", use: "pane card / surface hairline", color: "var(--tasty-border-default)" }, { tok: "--tasty-preset-leaf-label-fg", use: "summary label", color: "var(--tasty-text-muted)" }, { tok: "--tasty-preset-leaf-value-fg", use: "summary value", color: "var(--tasty-text-secondary)" }]} />
          <Do><b>Do</b> separate the two split levels by <b>weight</b>: a heavy gap+border for pane (upper) splits, a hairline for surface (lower) splits — so the hierarchy reads at a glance even at thumbnail scale.</Do>
          <Dont><b>Don't</b> render surface <i>contents</i> (a program's live output). A leaf shows only its <b>kind</b> + its <b>configured fields</b> (cwd / startup / file / url) — the preview is about <i>structure &amp; config</i>, not runtime data. Empty fields hide their row; below ~96×72px the summary drops, below 46px the label drops too. And never draw a <b>form</b> inside a leaf — parameters are edited on the settings screen.</Dont>
        </Spec>

        <Spec title="WYSIWYG edit mode — direct manipulation of STRUCTURE"
          when={<>The <b>Edit</b> button turns the same preview editable in place with the manipulation grammar of a real tasty pane — for <b>structure</b>. <b>Split by edge:</b> hover a surface's <b>~30% edge band</b> (left / right / top / bottom) to preview a split there, then click to add a new surface on that side. <b>Click the center</b> of a surface to <b>select</b> it — a <b>2px accent outline</b> plus two handles: <b>settings</b> (gear) and <b>remove</b>. The gear, or a <b>double-click</b>, opens the <b>surface settings screen</b> (next spec) for that leaf's parameters; the selection stays the target of the structure shortcuts, which is why a single click never leaves the preview (decision A1). <b>Tabs:</b> the mini strip gets a trailing <code>+</code> add-tab and a hover/active <b>close ×</b> per tab (hidden on the last remaining tab). Structure changes <b>save automatically</b>. Try it below — edge-split, tab add/close, delete, and the gear all work.</>}>
          <Stage variant="solo" style={{ padding: 18, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 14, alignItems: "stretch" }}>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
              <div>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>View — read-only</div>
                <LivePreview scope="pane" build={buildEditDemo} edit={false} height={232} />
              </div>
              <div>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>Edit — hover an edge to split, click center to select, gear / double-click for settings</div>
                <LivePreview scope="pane" build={buildEditDemo} edit={true} height={232} />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["split", "hover 30% edge band → click"], ["selected", "2px accent inset + gear · remove handles"], ["settings", "gear or double-click → settings screen"], ["tabs", "trailing + add · hover/active × close"], ["last tab", "close × hidden (guard)"], ["persistence", "structure auto-saves · parameters via OK"]]}
            tokens={[{ tok: "--tasty-preset-split-zone-bg", use: "edge split band", color: "var(--tasty-accent-primary)" }, { tok: "--tasty-preset-split-zone-border", use: "split line", color: "var(--tasty-accent-primary)" }, { tok: "--tasty-accent-primary", use: "selection outline", color: "var(--tasty-accent-primary)" }, { tok: "--tasty-accent-danger", use: "remove handle", color: "var(--tasty-accent-danger)" }]} />
          <Note>Split direction follows the edge: left/top add the new surface <i>before</i> (left / above), right/bottom <i>after</i>. Below <b>~46px</b> on an axis that axis's bands drop out (degrade) so tiny leaves stay selectable. Every leaf — selected or not — keeps its kind + <b>field-value summary</b>; nothing is ever drawn <i>inside</i> a cell that could clip. Empty-list scope shows <span className="ic">"No presets saved yet."</span> in the left list.</Note>
        </Spec>

        <Spec title="Surface settings screen — parameters as a draft"
          when={<>Editing a leaf's <b>parameters</b> (kind · <code>cwd</code> · startup · whatever the kind declares) used to open a form <i>inside</i> the leaf cell, which clipped in the six-surface workspace (the input and startup command fell outside the box). Now the settings screen <b>takes over the whole detail column</b> — toolbar and preview — as three stacked boxes: a <b>header</b> (kind icon in its accent + kind name · mono breadcrumb <code>preset › pane N › tab › surface k</code> · an <b>unsaved</b> dot once the draft differs), a <b>scrolling body</b> (one-column form, label over full-width input, capped at 460px; <b>Kind</b> first, then the kind's fields — changing Kind swaps the fields in place, keeping keys both kinds share), and a <b>fixed footer</b> (1px rule, right-aligned <b>Cancel</b> ghost · <b>OK</b> primary — the Settings-window footer grammar; product labels <span className="ic">취소 / 확인</span> via <code>button.cancel</code> / <code>button.ok</code>). The screen is a <b>draft</b>: OK applies the whole surface at once and auto-saves, Cancel discards. OK is <b>disabled until something changed</b> (decision E). While it is open the left list and L1 scope tabs <b>dim and ignore input</b> (decision C) — leaving would silently drop the draft; an outside selection change (context-menu “save as preset”, window close) drops it like Cancel, without a prompt. Save failure: the screen and draft stay put, the error goes to a toast. Try a frame: change Kind, hit Esc / Enter.</>}>
          <Stage variant="solo" style={{ padding: 18, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 16, alignItems: "stretch" }}>
            <div style={{ display: "flex", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
              <SettingsDemo label="1 · terminal — 2 fields" width={280} height={300} path={["six", "pane 1", "code", "surface 2"]}
                surface={{ kind: "terminal", cwd: "~/tasty", startup: "cargo watch -x run" }} />
              <SettingsDemo label="2 · markdown — 1 field + Browse" width={280} height={300} path={["six", "pane 2", "ops", "surface 3"]}
                surface={{ kind: "markdown", file: "docs/runbook.md" }} />
              <SettingsDemo label="3 · plugin kind — 7 fields, body scrolls (header + footer stay)" width={280} height={300} scrollTo={120} path={["six", "pane 2", "ops", "surface 2"]}
                surface={{ kind: "plugin:portscan", host: "127.0.0.1", range: "3000-3999", protocol: "tcp", interval: "1000", timeout: "200", output: "", notify: "new port" }} />
                          <SettingsDemo label="4 · kind changed → unsaved dot, OK enabled" width={280} height={300} path={["claude", "pane 1", "edit", "surface 1"]}
                orig={{ kind: "editor", cwd: "~/tasty/src" }} surface={{ kind: "terminal", cwd: "~/tasty/src", startup: "" }} />
              <SettingsDemo label="5 · narrow detail (220px) — breadcrumb ellipsizes, Browse stays" width={220} height={300} path={["claude", "pane 2", "preview", "surface 1"]}
                surface={{ kind: "markdown", file: "docs/architecture.md" }} />
            </div>
          </Stage>
          <Meta
            specs={[["boxes", "header / scroll body / fixed footer"], ["header", "44px — same as the toolbar it replaces"], ["body", "16px padding · 12px field gap · max 460"], ["footer", "52px fixed · 0 14px · Cancel ghost + OK primary"], ["OK", "disabled until draft ≠ saved"], ["keys", "Esc = Cancel · Enter in an input = OK"], ["structure keys", "inert while open (preview hidden)"], ["after OK/Cancel", "back to preview, leaf stays selected"]]}
            tokens={[{ tok: "--tasty-preset-cfg-header-height", use: "header (44)", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-preset-cfg-footer-height", use: "footer (52)", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-preset-cfg-form-max-width", use: "form column cap (460)", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-preset-cfg-draft-fg", use: "unsaved dot", color: "var(--tasty-accent-warning)" }, { tok: "--tasty-preset-cfg-dim-opacity", use: "list + scope while open", color: "var(--tasty-bg-sidebar)" }]} />
          <Do><b>Do</b> keep the three boxes independent: only the body scrolls; header and footer never move, so Cancel / OK sit in the same place for a two-field terminal and a twenty-field plugin.</Do>
          <Dont><b>Don't</b> show “saved automatically” while the screen is open — it is not true for the draft. The toolbar is replaced wholesale (decision B) so the two save models never appear side by side; the header's <b>unsaved</b> dot is the only persistence cue on this screen.</Dont>
        </Spec>
      </Section>
    );
  }

  // small labelled wrapper for the 3-scope demo grid
  function ScopeDemo({ label, sub, scope, build }) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 600 }}>{label}</span>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10.5, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{sub}</span>
        </div>
        <LivePreview scope={scope} build={build} height={236} />
      </div>
    );
  }

  window.PresetEditor = { Section: PresetEditorSection, NAV_ITEM: { id: "preseteditor", label: "Preset editor" } };
})();
