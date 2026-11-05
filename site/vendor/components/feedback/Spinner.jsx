import React from "react";

/**
 * Tasty Spinner — a small indeterminate progress indicator for short
 * background work (port scans, plugin installs, surface attach). A thin
 * rotating arc on a faint track. Honors prefers-reduced-motion by falling
 * back to three static dots instead of a spinning ring.
 */

const CSS = `
.tasty-spinner { display: inline-block; flex: none; color: var(--tasty-spinner-indicator);
  width: var(--tasty-spinner-size, var(--tasty-size-16)); height: var(--tasty-spinner-size, var(--tasty-size-16)); }
.tasty-spinner svg { display: block; width: 100%; height: 100%; animation: tasty-spin 0.9s linear infinite; }
.tasty-spinner__track { opacity: 0.22; }
.tasty-spinner__arc { opacity: 1; }
@keyframes tasty-spin { to { transform: rotate(360deg); } }

.tasty-spinner--dots { position: relative; }
.tasty-spinner--dots svg { display: none; }
.tasty-spinner--dots::before {
  content: ""; display: block; width: 100%; height: 100%;
  background:
    radial-gradient(circle, currentColor 42%, transparent 46%) 0 50% / 28% 28% no-repeat,
    radial-gradient(circle, currentColor 42%, transparent 46%) 50% 50% / 28% 28% no-repeat,
    radial-gradient(circle, currentColor 42%, transparent 46%) 100% 50% / 28% 28% no-repeat;
}

.tasty-spinner--paused svg { animation: none; }

@media (prefers-reduced-motion: reduce) {
  .tasty-spinner svg { animation: none; }
  .tasty-spinner { position: relative; }
  .tasty-spinner svg { display: none; }
  .tasty-spinner::before {
    content: ""; display: block; width: 100%; height: 100%;
    background:
      radial-gradient(circle, currentColor 42%, transparent 46%) 0 50% / 28% 28% no-repeat,
      radial-gradient(circle, currentColor 42%, transparent 46%) 50% 50% / 28% 28% no-repeat,
      radial-gradient(circle, currentColor 42%, transparent 46%) 100% 50% / 28% 28% no-repeat;
  }
  .tasty-spinner.tasty-spinner--rm-arc svg { display: block; }
  .tasty-spinner.tasty-spinner--rm-arc::before { content: none; }
}
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-spinner-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Spinner({ size = 16, stroke = 2, label = null, paused = false, reducedMotion = "dots", className = "", style, ...rest }) {
  ensureCss();
  // paused = force the still arc (no rotation) · reducedMotion "arc" = under prefers-reduced-motion keep the
  // still arc instead of the 3-dot fallback (used by LoadingOverlay, 2026-10-16)
  const cls = ["tasty-spinner", paused && "tasty-spinner--paused", reducedMotion === "arc" && "tasty-spinner--rm-arc", className].filter(Boolean).join(" ");
  const r = 12 - stroke; // keep the stroke inside the 24-unit viewBox
  return (
    <span
      className={cls}
      role="status"
      aria-label={typeof label === "string" ? label : "Loading"}
      style={{ ["--tasty-spinner-size"]: typeof size === "number" ? size + "px" : size, ...style }}
      {...rest}
    >
      <svg viewBox="0 0 24 24" fill="none">
        {/* The arc carries the meaning. The track is an ornament with no grade: RULES.md, Spinner. */}
        <circle className="tasty-spinner__track" cx="12" cy="12" r={r}
          stroke="currentColor" strokeWidth={stroke} data-ink-grade="none" data-ink-why="ornament" />
        <path className="tasty-spinner__arc"
          d={`M12 ${12 - r} a ${r} ${r} 0 0 1 ${r} ${r}`}
          stroke="currentColor" strokeWidth={stroke} strokeLinecap="round" />
      </svg>
    </span>
  );
}
