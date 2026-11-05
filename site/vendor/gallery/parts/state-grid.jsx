// Tasty Gallery: the fixed-state specimen. See gallery/README.md, "Fixed states".
// Renders one contract of tokens/ground-ink-rules.json as a grid of frozen cells, host by state.
// Nothing here restyles a component: mirrorPseudoStates copies the component's own :hover, :active and :focus
// rules onto a data attribute, so a frozen cell can only show what the live component shows.
import React, { useEffect, useRef, useState } from "react";
import { components as ROWS, grounds as GROUNDS } from "../../tokens/ground-ink-rules.json";
import { cellFor, FORCE_ALSO } from "./contract-cells.jsx";

const FORCE = [
  [/:hover\b/g, '[data-force~="hover"]'],
  [/:active\b/g, '[data-force~="press"]'],
  [/:focus-visible\b/g, '[data-force~="focus"]'],
  [/:focus-within\b/g, '[data-force~="focus"]'],
  [/:focus\b/g, '[data-force~="focus"]'],
];
const seen = new WeakSet();

function mirrorRules(group) {
  // Back to front, so an insert does not move a rule not yet read. Each copy sits right after its original
  // and has the same specificity, so the cascade order of the source stays.
  for (let i = group.cssRules.length - 1; i >= 0; i -= 1) {
    const rule = group.cssRules[i];
    if (rule.styleSheet) { mirrorRules(rule.styleSheet); continue; }
    if (rule.cssRules && rule.cssRules.length && !rule.selectorText) { mirrorRules(rule); continue; }
    if (!rule.selectorText || !/:(hover|active|focus)/.test(rule.selectorText)) continue;
    let selector = rule.selectorText;
    FORCE.forEach(([from, to]) => { selector = selector.replace(from, to); });
    // A pseudo-element such as ::-webkit-scrollbar-thumb takes no attribute after it. That rule has no frozen form.
    try { group.insertRule(selector + rule.cssText.slice(rule.cssText.indexOf("{")), i + 1); } catch (e) { console.warn("state mirror skipped", selector); }
  }
}

export function mirrorPseudoStates() {
  Array.from(document.styleSheets).forEach((sheet) => {
    if (seen.has(sheet)) return;
    seen.add(sheet);
    try { mirrorRules(sheet); } catch (e) { console.warn("state mirror skipped a sheet", e); }
  });
}

export const STATE_ORDER = ["rest", "hover", "press", "focus", "selected", "selected+hover", "selected+press", "keyboard-active", "disabled", "invalid", "readonly"];
const FORCED = { hover: "hover", press: "hover press", focus: "focus", "selected+hover": "hover", "selected+press": "hover press" };
const GROUND = Object.fromEntries(GROUNDS.map((g) => [g.id, g]));

function StateCell({ contract, state, host, children }) {
  const ref = useRef(null);
  useEffect(() => {
    mirrorPseudoStates();
    const root = ref.current;
    if (!root) return;
    const also = FORCE_ALSO[contract];
    const targets = [...Array.from(root.children), ...(also ? Array.from(root.querySelectorAll(also)) : [])];
    targets.forEach((el) => {
      if (FORCED[state]) el.setAttribute("data-force", FORCED[state]);
      el.setAttribute("aria-hidden", "true");
    });
    root.querySelectorAll("button, input, select, textarea, a[href], [tabindex]").forEach((el) => el.setAttribute("tabindex", "-1"));
  }, [state, children]);
  return (
    <td className="sg-cell" data-contract={contract} data-state={state} data-host={host}
      style={{ background: `var(--tasty-${GROUND[host].token})` }}>
      <div ref={ref} className="sg-frozen">{children}</div>
    </td>
  );
}

export function StateGrid({ contract, label }) {
  const render = cellFor(contract);
  const mine = ROWS.filter((r) => r.component === contract);
  const states = STATE_ORDER.filter((s) => mine.some((r) => r.state === s));
  const hosts = Array.from(new Set(mine.map((r) => r.host)));
  const has = (host, state) => mine.some((r) => r.host === host && r.state === state);
  return (
    <div className="sg" role="group" aria-label={`${contract} states`}>
      <div className="sg-title">{label || contract}</div>
      <div className="sg-scroll" tabIndex={0}>
        <table>
          <thead>
            <tr><th className="sg-corner">host</th>{states.map((s) => <th key={s}>{s}</th>)}</tr>
          </thead>
          <tbody>
            {hosts.map((host) => (
              <tr key={host}>
                <th scope="row">{host}</th>
                {states.map((state) => (has(host, state)
                  ? <StateCell key={state} contract={contract} state={state} host={host}>{render(state, host)}</StateCell>
                  : <td key={state} className="sg-none" />))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// Reads the value through a throwaway element, so a table cannot drift from the tokens.
export function resolveToken(name, theme) {
  const el = document.createElement("div");
  if (theme) el.setAttribute("data-theme", theme);
  el.style.cssText = `position:absolute;visibility:hidden;color:var(--tasty-${name})`;
  document.body.appendChild(el);
  const cs = getComputedStyle(el);
  let out = cs.getPropertyValue(`--tasty-${name}`).trim();
  if (theme) {
    const m = /rgba?\((\d+), (\d+), (\d+)(?:, ([\d.]+))?\)/.exec(cs.color) || /color\(srgb ([\d.]+) ([\d.]+) ([\d.]+)(?: \/ ([\d.]+))?/.exec(cs.color);
    if (m) {
      const to255 = (n) => (cs.color.startsWith("color") ? Math.round(Number(n) * 255) : Number(n));
      out = "#" + m.slice(1, 4).map((n) => to255(n).toString(16).padStart(2, "0")).join("") + (m[4] !== undefined && Number(m[4]) < 1 ? ` / ${Number(m[4])}` : "");
    }
  }
  el.remove();
  return out;
}

function ResolvedColor({ name, theme }) {
  const [v, set] = useState("");
  useEffect(() => { set(resolveToken(name, theme)); }, [name, theme]);
  return <>{v}</>;
}

// Part, token, states and the value in each theme, generated from the contract's rows.
export function ContractTable({ contract }) {
  const groups = new Map();
  ROWS.filter((r) => r.component === contract).forEach((r) => {
    const key = r.part + "\u0000" + r.token;
    if (!groups.has(key)) groups.set(key, { part: r.part, token: r.token, states: [] });
    const g = groups.get(key);
    if (!g.states.includes(r.state)) g.states.push(r.state);
  });
  return (
    <div className="spec-table" tabIndex={0}>
      <table>
        <thead><tr><th>{contract}</th><th>Token</th><th>State</th><th>Mocha</th><th>Latte</th></tr></thead>
        <tbody>
          {Array.from(groups.values()).map((g) => (
            <tr key={g.part + g.token}>
              <td>{g.part}</td>
              <td className="m">{g.token}</td>
              <td className="d">{g.states.join(", ")}</td>
              <td className="m"><ResolvedColor name={g.token} theme="mocha" /></td>
              <td className="m"><ResolvedColor name={g.token} theme="latte" /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
