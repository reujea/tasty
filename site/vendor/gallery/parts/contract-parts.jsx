// Tasty Gallery: a frozen cell for a part with no component. See gallery/README.md, "Fixed states".
// The cell is written with the contract's tokens directly, on the ground the file names for each row.
import React from "react";
import { components as ROWS, grounds as GROUNDS, roles as ROLES } from "../../tokens/ground-ink-rules.json";

const GROUND = Object.fromEntries(GROUNDS.map((g) => [g.id, g]));
const ROLE = Object.fromEntries(ROLES.map((r) => [r.id, r]));

function Ground({ id, children }) {
  const g = GROUND[id];
  if (!g) return children;
  const fill = <span style={{ display: "inline-flex", background: `var(--tasty-${g.token})` }}>{children}</span>;
  const parent = g.parents && g.parents[0];
  return parent && GROUND[parent] ? <Ground id={parent}>{fill}</Ground> : fill;
}

function Part({ part, token, kind }) {
  const color = `var(--tasty-${token})`;
  if (kind === "edge") {
    return <span className="sg-part sg-part--edge" data-part={part} style={{ borderColor: color }}>{part}</span>;
  }
  if (kind === "glyph") {
    return (
      <span className="sg-part" data-part={part} style={{ color }}>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-label={part}><path d="M5 12h14M12 5v14" /></svg>
      </span>
    );
  }
  return <span className="sg-part" data-part={part} style={{ color }}>{part}</span>;
}

// One cell: every row of the contract for this state and host.
export const tokenCell = (contract) => (state, host) => {
  const rows = ROWS.filter((r) => r.component === contract && r.state === state && r.host === host);
  return rows.map((r) => (
    <Ground key={r.part} id={r.ground}><Part part={r.part} token={r.token} kind={(ROLE[r.role] || {}).kind} /></Ground>
  ));
};
