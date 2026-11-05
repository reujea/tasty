import React from "react";
import RULES_URL from "../../tokens/ground-ink-rules.json?url";
import { Icon as RIcon } from "../../components/index.js";
import { Section, Spec, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Rules. Renders every ground × role cell of tokens/ground-ink-rules.json with the real
// tokens: the cell paints the ground, then the ink. Layout here is gallery chrome; colors come only from tokens.


const { useState: useS, useEffect: useE } = React;

const TABS = [
  ["Neutral at rest", (g) => g.cls === "neutral" && !g.parents],
  ["Hover and press", (g) => g.cls === "neutral-state"],
  ["Selected", (g) => g.id === "active" || g.cls === "active-state"],
  ["Filled", (g) => g.cls === "fill"],
  ["Tracks", (g) => g.cls === "track"],
  ["Status tints", (g) => g.cls === "tint" && !g.parents],
  ["Tint boxes", (g) => g.cls === "tint" && !!g.parents],
  ["Terminal layers", (g) => g.cls === "termlayer" || g.cls === "match"],
  ["Scrim", (g) => g.cls === "scrim"],
];

function useTheme() {
  const read = () => (typeof document !== "undefined" && document.documentElement.getAttribute("data-theme") === "latte" ? "latte" : "mocha");
  const [t, set] = useS(read());
  useE(() => {
    const o = new MutationObserver(() => set(read()));
    o.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => o.disconnect();
  }, []);
  return t;
}

function Ground({ id, map, children, style }) {
  const g = map[id];
  const inner = <div style={{ background: `var(--tasty-${g.token})`, ...style }}>{children}</div>;
  return g.parents ? <Ground id={g.parents[0]} map={map}>{inner}</Ground> : inner;
}

function Ink({ role, token }) {
  const c = `var(--tasty-${token})`;
  if (role.kind === "text") return <span data-ink-grade={token.includes("disabled") ? "none" : undefined} data-ink-why={token.includes("disabled") ? "disabled" : undefined} style={{ color: c, fontSize: "var(--tasty-font-size-body)" }}>Aa</span>;
  if (role.kind === "glyph") return <span data-ink-grade={token === "accent-decorative" ? "none" : undefined} data-ink-why={token === "accent-decorative" ? "ornament" : undefined} style={{ color: c, display: "inline-flex" }}><RIcon name="check" size="var(--tasty-icon-size-sm)" /></span>;
  return <span style={{ display: "block", width: "var(--tasty-size-24)", height: "var(--tasty-size-12)", border: `var(--tasty-border-width) solid ${c}`, borderRadius: "var(--tasty-radius-sm)" }} />;
}

// One row of the site section. The sample paints the row's ground, then its ink. A gradient ground paints its strong end.
function SiteSample({ p, gid, map }) {
  const c = `var(--tasty-${p.token})`;
  const ink = p.kind === "mark"
    ? <span data-audit-exempt="brand" style={{ color: c, fontSize: "var(--tasty-font-size-body)" }}>.</span>
    : p.kind === "ornament"
      ? <span data-ink-grade="none" data-ink-why="ornament" style={{ color: c, fontSize: "var(--tasty-font-size-body)" }}>/</span>
      : p.kind === "glyph"
        ? <span style={{ color: c, display: "inline-flex" }}><RIcon name="check" size="var(--tasty-icon-size-sm)" /></span>
        : p.kind === "edge"
          ? <span style={{ display: "block", width: "var(--tasty-size-24)", height: "var(--tasty-size-12)", border: `var(--tasty-border-width) solid ${c}`, borderRadius: "var(--tasty-radius-sm)" }} />
          : <span style={{ color: c, fontSize: "var(--tasty-font-size-body)" }}>Aa</span>;
  return (
    <Ground id={gid} map={map} style={{ width: cellW, height: "var(--tasty-control-height)", display: "flex", alignItems: "center", justifyContent: "center" }}>{ink}</Ground>
  );
}

const cellW = "var(--tasty-size-88)";
const th = { position: "sticky", top: 0, background: "var(--tasty-bg-sidebar)", color: "var(--tasty-text-secondary)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", fontWeight: "var(--tasty-font-weight-normal)", padding: "var(--tasty-space-xs)", textAlign: "left", minWidth: cellW, width: cellW, borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", zIndex: 1, wordBreak: "break-all", verticalAlign: "bottom" };
const rowh = { position: "sticky", left: 0, background: "var(--tasty-bg-sidebar)", color: "var(--tasty-text-primary)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", padding: "var(--tasty-space-xs) var(--tasty-space-sm)", whiteSpace: "nowrap", zIndex: 1, textAlign: "left", fontWeight: "var(--tasty-font-weight-normal)" };

function Cell({ c, role, map, theme }) {
  if (c.result === "forbidden") {
    return <td title={c.reason} style={{ padding: "var(--tasty-space-xs)", color: "var(--tasty-text-muted)", fontSize: "var(--tasty-font-size-micro)", verticalAlign: "top" }}>forbidden</td>;
  }
  return (
    <td style={{ padding: "var(--tasty-space-xs)", verticalAlign: "top" }} title={c.result}>
      <Ground id={c.ground} map={map} style={{ height: "var(--tasty-control-height)", display: "flex", alignItems: "center", justifyContent: "center" }}>
        <Ink role={role} token={c.result} />
      </Ground>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)", marginTop: "var(--tasty-size-2)" }}>
        {role.grade ? c.contrast[theme].toFixed(2) : "no target"}
      </div>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)", wordBreak: "break-all" }}>{c.result}</div>
    </td>
  );
}

function Rules() {
  const [data, setData] = useS(null);
  const [err, setErr] = useS(null);
  const theme = useTheme();
  useE(() => {
    fetch(RULES_URL).then((r) => r.json()).then(setData).catch((e) => setErr(String(e)));
  }, []);
  if (err) return <Note>Could not load tokens/ground-ink-rules.json: {err}</Note>;
  if (!data) return <>{page.nav.map((n) => <Section key={n.id} id={n.id} title={n.label} />)}<Note>Loading the table.</Note></>;
  const map = Object.fromEntries(data.grounds.map((g) => [g.id, g]));
  const siteMap = { ...map, ...Object.fromEntries(data.site.grounds.map((g) => [g.id, g])) };
  const cells = {};
  data.matrix.forEach((c) => { cells[c.ground + "|" + c.role] = c; });
  const total = data.matrix.length;
  const tokenCells = data.matrix.filter((c) => c.result !== "forbidden").length;
  return (
    <>
      <Section id="table" title="Ground by role">
        <Spec id="how" title="Every cell, in the current theme" when={`${total} cells: ${tokenCells} name a token, ${total - tokenCells} are forbidden. Each sample paints the ground and then the ink, with the same tokens the app reads. The number is the measured contrast. Hover a forbidden cell for the reason. Switch the theme at the top right.`}>
          {TABS.map(([label, pick]) => {
            const shown = data.grounds.filter(pick);
            return (
          <figure className="fixed-view" data-view={label} key={label}>
          <figcaption>{label}</figcaption>
          <div className="tasty-scroll" style={{ marginTop: "var(--tasty-space-md)", overflow: "auto", maxHeight: "var(--tasty-size-1100)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-panel)" }}>
            <table style={{ borderCollapse: "collapse" }}>
              <thead>
                <tr>
                  <th style={{ ...th, left: 0, zIndex: 2, minWidth: "auto", width: "auto" }}>role / ground</th>
                  {shown.map((g) => <th key={g.id} style={th} title={g.token}>{g.id}</th>)}
                </tr>
              </thead>
              <tbody>
                {data.roles.map((r) => (
                  <tr key={r.id}>
                    <th style={rowh} title={r.desc}>{r.id} <span style={{ color: "var(--tasty-text-muted)" }}>{r.grade || "none"}</span></th>
                    {shown.map((g) => <Cell key={g.id} c={cells[g.id + "|" + r.id]} role={r} map={map} theme={theme} />)}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          </figure>
            );
          })}
        </Spec>
      </Section>
      <Section id="site" title="Site pages">
        <Spec id="site-places" title="One ink per place, on each ground it stands on" when="The site's own header, sidebar, pages and footer, in the current theme. RULES.md section 15 says which values are new and why. The number is the lowest across the row's grounds.">
          <table style={{ borderCollapse: "collapse", fontSize: "var(--tasty-font-size-caption)" }}>
            <tbody>
              {data.site.places.map((p) => (
                <tr key={p.id}>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)", color: "var(--tasty-text-primary)" }}>{p.name}</td>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)" }}>
                    <span style={{ display: "inline-flex", gap: "var(--tasty-space-xs)" }}>
                      {p.grounds.map((gid) => <SiteSample key={gid} p={p} gid={gid} map={siteMap} />)}
                    </span>
                  </td>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>{p.token}</td>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>{p.grade ? p.contrast[theme].toFixed(2) : "no target"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Spec>
      </Section>
      <Section id="groups" title="Colors told apart">
        <Spec id="distinct" title="Closest pair in each group" when="A pair under OKLab ΔE 0.08 must have a second channel.">
          <table style={{ borderCollapse: "collapse", fontSize: "var(--tasty-font-size-caption)" }}>
            <tbody>
              {data.distinct.filter((d) => d.theme === theme).map((d) => (
                <tr key={d.group}>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)", color: "var(--tasty-text-primary)" }}>{d.group}</td>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)" }}>
                    <span style={{ display: "inline-flex", gap: "var(--tasty-space-xs)" }}>
                      {d.pair.map((p) => <span key={p} title={p} style={{ width: "var(--tasty-size-12)", height: "var(--tasty-size-12)", borderRadius: "var(--tasty-radius-pill)", background: `var(--tasty-${p})` }} />)}
                    </span>
                  </td>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>{d.minDeltaE.toFixed(3)}</td>
                  <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)", color: "var(--tasty-text-secondary)" }}>{d.secondChannel || "Hue is enough."}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Spec>
      </Section>
    </>
  );
}

export const page = {
  active: "rules",
  nav: [{ id: "table", label: "Ground by role" }, { id: "site", label: "Site pages" }, { id: "groups", label: "Colors told apart" }],
  head: { title: "Ground and ink rules", intro: "The table behind every color choice. The rules are in RULES.md and the data is in tokens/ground-ink-rules.json." },
  content: <Rules />,
};

export default function GalleryRoute() {
  return page.content;
}
