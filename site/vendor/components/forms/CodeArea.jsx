import React from "react";

/**
 * Tasty CodeArea — multi-line monospace text field with a line-number gutter
 * (2026-10-06, first consumer: Settings › Hook Handlers IpcSequence editor).
 * Same box as Input (surface-raised, 1px border-default, radius, focus edge + ring);
 * text = mono caption like Input mono. Lines never wrap (wrap="off"); the box grows
 * with the content up to --tasty-codearea-max-height, then scrolls.
 *
 * invalid   — danger border (same as Input invalid).
 * errorLine — 1-based line to mark: its gutter number turns danger and the line
 *             takes a tint-fill-alpha danger band. One line at a time (first error).
 * disabled  — neutral disabled box + disabled ink, like Input.
 */

const CSS = `
.tasty-codearea { position: relative; display: flex; width: 100%; max-height: var(--tasty-codearea-max-height); overflow: auto;
  border: var(--tasty-border-width) solid var(--tasty-border-control); border-radius: var(--tasty-radius);
  background: var(--tasty-surface-field); color: var(--tasty-text-primary);
  font-family: var(--tasty-font-mono); font-size: var(--tasty-codearea-font-size); line-height: var(--tasty-codearea-line-height);
  transition: border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui), box-shadow var(--tasty-motion-ui-fast) var(--tasty-ease-ui); }
.tasty-codearea:focus-within { border-color: var(--tasty-border-focus); box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-border-focus); }
.tasty-codearea--invalid { border-color: var(--tasty-accent-danger); }
.tasty-codearea--invalid:focus-within { box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-accent-danger); }
.tasty-codearea__gutter { flex: none; position: sticky; left: 0; z-index: 1; min-width: var(--tasty-codearea-gutter-width);
  padding: var(--tasty-codearea-padding-y) var(--tasty-space-xs); box-sizing: border-box; text-align: right; user-select: none;
  background: var(--tasty-codearea-gutter-bg); color: var(--tasty-codearea-gutter-fg); border-right: var(--tasty-border-width) solid var(--tasty-codearea-gutter-border); }
.tasty-codearea__gutter span { display: block; }
.tasty-codearea__gutter span.is-error { color: var(--tasty-codearea-error-fg); font-weight: var(--tasty-font-weight-semibold); }
.tasty-codearea__body { position: relative; flex: 1; min-width: 0; }
.tasty-codearea__band { position: absolute; left: 0; right: 0; pointer-events: none;
  top: calc(var(--tasty-codearea-padding-y) + (var(--ln) - 1) * var(--tasty-codearea-font-size) * var(--tasty-codearea-line-height));
  height: calc(var(--tasty-codearea-font-size) * var(--tasty-codearea-line-height));
  background: color-mix(in srgb, var(--tasty-accent-danger) calc(var(--tasty-tint-fill-alpha) * 100%), transparent); }
.tasty-codearea textarea { position: relative; display: block; width: 100%; box-sizing: border-box; margin: 0; border: 0; outline: 0; resize: none;
  padding: var(--tasty-codearea-padding-y) var(--tasty-codearea-padding-x); background: transparent; color: inherit; font: inherit;
  white-space: pre; overflow: hidden; }
.tasty-codearea textarea::placeholder { color: var(--tasty-text-placeholder); }
.tasty-codearea[data-disabled="true"] { background: var(--tasty-state-disabled-fill); border-color: var(--tasty-state-disabled-border); pointer-events: none; }
.tasty-codearea[data-disabled="true"] textarea, .tasty-codearea[data-disabled="true"] .tasty-codearea__gutter { color: var(--tasty-state-disabled-fg); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-codearea-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function CodeArea({
  value, defaultValue = "", onChange, minRows = 4, invalid = false, errorLine = null,
  disabled = false, className = "", style, ...rest
}) {
  ensureCss();
  const [inner, setInner] = React.useState(defaultValue);
  const text = value !== undefined ? value : inner;
  const lines = Math.max(minRows, text.split("\n").length);
  const cls = ["tasty-codearea", invalid || errorLine ? "tasty-codearea--invalid" : "", className].filter(Boolean).join(" ");
  return (
    <div className={cls} data-disabled={disabled} style={style}>
      <div className="tasty-codearea__gutter" aria-hidden="true">
        {Array.from({ length: lines }, (_, i) => <span key={i} className={errorLine === i + 1 ? "is-error" : ""}>{i + 1}</span>)}
      </div>
      <div className="tasty-codearea__body">
        {errorLine && <div className="tasty-codearea__band" style={{ "--ln": errorLine }} />}
        <textarea wrap="off" spellCheck={false} rows={lines} disabled={disabled} value={text}
          aria-invalid={invalid || !!errorLine}
          onChange={(e) => { if (value === undefined) setInner(e.target.value); onChange && onChange(e); }} {...rest} />
      </div>
    </div>
  );
}
