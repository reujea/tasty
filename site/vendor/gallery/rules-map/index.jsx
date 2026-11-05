import React from "react";
import RULES_URL from "../../tokens/ground-ink-rules.json?url";
import { Section, Spec, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Location map. Each location paints its ground and the child grounds it may hold, in both themes.
// Ink comes from the text-body cell of tokens/ground-ink-rules.json. Colors come only from tokens.

const { useState: useS2, useEffect: useE2 } = React;

function GBox({ id, map, cells, style }) {
  const g = map[id];
  const c = cells[id + "|text-body"];
  const tok = c && c.result !== "forbidden" ? c.result : null;
  const inner = (
    <div style={{ background: `var(--tasty-${g.token})`, padding: "var(--tasty-space-xs) var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", minHeight: "var(--tasty-size-24)", display: "flex", alignItems: "center", ...style }}>
      {tok ? <span style={{ color: `var(--tasty-${tok})` }}>{id}</span> : <span aria-hidden="true" style={{ width: "var(--tasty-size-24)" }} />}
    </div>
  );
  if (!tok) return <div title={id} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{inner}<span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{id}, no text</span></div>;
  if (!tok) return <div title={id} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{inner}<span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{id}, no text</span></div>;
  if (!tok) return <div title={id} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{inner}<span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{id}, no text</span></div>;
  if (!tok) return <div title={id} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{inner}<span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{id}, no text</span></div>;
  if (!tok) return <div title={id} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{inner}<span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{id}, no text</span></div>;
  if (!tok) return <div title={id} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{inner}<span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{id}, no text</span></div>;
  if (!tok) return <div title={id} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{inner}<span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{id}, no text</span></div>;
  if (!g.parents) return inner;
  const p = map[g.parents[0]];
  return p.parents
    ? <div style={{ background: `var(--tasty-${map[p.parents[0]].token})` }}><div style={{ background: `var(--tasty-${p.token})` }}>{inner}</div></div>
    : <div style={{ background: `var(--tasty-${p.token})` }}>{inner}</div>;
}

function Loc({ l, map, cells, theme }) {
  return (
    <div data-theme={theme} style={{ flex: 1, minWidth: 0, background: "var(--tasty-bg-app)", padding: "var(--tasty-space-sm)", borderRadius: "var(--tasty-radius)" }}>
      <GBox id={l.ground} map={map} cells={cells} />
      <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-xs)", marginTop: "var(--tasty-space-xs)" }}>
        {l.children.map((k) => <GBox key={k} id={k} map={map} cells={cells} style={{ border: "var(--tasty-border-width) solid var(--tasty-separator)", borderRadius: "var(--tasty-radius-sm)" }} />)}
      </div>
    </div>
  );
}

function RulesMap() {
  const [data, setData] = useS2(null);
  const [err, setErr] = useS2(null);
  useE2(() => { fetch(RULES_URL).then((r) => r.json()).then(setData).catch((e) => setErr(String(e))); }, []);
  if (err) return <Note>Could not load tokens/ground-ink-rules.json: {err}</Note>;
  if (!data) return <>{page.nav.map((n) => <Section key={n.id} id={n.id} title={n.label} />)}<Note>Loading the map.</Note></>;
  const map = Object.fromEntries(data.grounds.map((g) => [g.id, g]));
  const cells = {};
  data.matrix.forEach((c) => { cells[c.ground + "|" + c.role] = c; });
  return (
    <Section id="map" title="Every location, Mocha then Latte">
      {data.locations.map((l) => (
        <Spec key={l.id} id={l.id} title={l.name} when={`Ground ${l.ground}. Nesting depth ${l.depth}. ${l.note}`}>
          <div style={{ display: "flex", gap: "var(--tasty-space-sm)", flexWrap: "wrap" }}>
            <Loc l={l} map={map} cells={cells} theme="mocha" />
            <Loc l={l} map={map} cells={cells} theme="latte" />
          </div>
        </Spec>
      ))}
    </Section>
  );
}

export const page = {
  active: "rules-map",
  nav: [{ id: "map", label: "All locations" }],
  head: { title: "Location map", intro: "See which ground belongs at each location and which grounds it can contain. Compare Mocha and Latte side by side to see the same structure in both themes." },
  content: <RulesMap />,
};

export default function GalleryRoute() {
  return page.content;
}
