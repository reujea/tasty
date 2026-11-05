// Tasty Gallery: the specimen vocabulary every page uses. The page shell (header, theme, sidebar, previous and next)
// belongs to the site layout. See gallery/README.md, "One specimen" and "Specs".
//   <Spec> one catalogued thing, in this order: heading and when to use, demo, states, views, specs.
//   <Stage> the live demo. <Views> what the demo hides, drawn as frames. <Meta> layout and tokens tables.
import React, { createContext, useContext, useEffect, useRef, useState } from "react";
import { StateGrid, ContractTable, resolveToken } from "./state-grid.jsx";

const StatesContext = createContext(null);

// value: { [specId]: [contract, ...] }, from the page's states.jsx.
export function PageStates({ value, children }) {
  return <StatesContext.Provider value={value}>{children}</StatesContext.Provider>;
}

export function Section({ id, title, children }) {
  return (
    <section className="g-section" id={id}>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

const flat = (children) => React.Children.toArray(children).flatMap((c) => (c && c.type === React.Fragment ? flat(c.props.children) : [c]));

export function Spec({ id, title, badges, when, children }) {
  const states = useContext(StatesContext);
  const kids = flat(children);
  const metas = kids.filter((c) => c && c.type === Meta);
  const views = kids.filter((c) => c && c.type === Views);
  const demo = kids.filter((c) => !(c && (c.type === Meta || c.type === Views)));
  const grids = (states && id && states[id]) || [];
  return (
    <article className="spec" id={id} data-spec={id}>
      <div className="spec-head">
        <h3>{title}{badges}</h3>
        {when && <p className="when">{when}</p>}
      </div>
      {demo.length > 0 && <div className="spec-demo">{demo}</div>}
      {grids.length > 0 && (
        <>
          <div className="part-h">States</div>
          {grids.map((contract) => <StateGrid key={contract} contract={contract} />)}
        </>
      )}
      {views}
      {(metas.length > 0 || grids.length > 0) && (
        <>
          <div className="part-h">Specs</div>
          <div className="spec-tables">
            {metas}
            {grids.map((contract) => <ContractTable key={contract} contract={contract} />)}
          </div>
        </>
      )}
    </article>
  );
}

// A state the demo hides behind a tab, a fold or an open menu. The check does not click, so it reads these.
export function Views({ children }) {
  return (
    <>
      <div className="part-h">Views</div>
      <div className="fixed-views">{children}</div>
    </>
  );
}

export function View({ name, label, children }) {
  return (
    <figure className="fixed-view" data-view={name}>
      <figcaption>{label || name}</figcaption>
      <div>{children}</div>
    </figure>
  );
}

// A live demo mounts once it scrolls near the viewport, so a page with many specimens renders a few stages at a time.
// With ?audit=1 every stage mounts at once and nothing else changes.
const LAZY_MARGIN = 700;
function useNearViewport(ref, eager) {
  const [shown, setShown] = useState(eager);
  useEffect(() => {
    if (shown) return undefined;
    const el = ref.current;
    if (!el) return undefined;
    if (new URLSearchParams(window.location.search).has("audit")) { setShown(true); return undefined; }
    const near = () => {
      const r = el.getBoundingClientRect();
      const vh = window.innerHeight || document.documentElement.clientHeight;
      return r.top < vh + LAZY_MARGIN && r.bottom > -LAZY_MARGIN;
    };
    if (near()) { setShown(true); return undefined; }
    let io = null;
    let last = 0;
    const onScroll = () => {
      const now = Date.now();
      if (now - last < 120) return;
      last = now;
      if (near()) { setShown(true); cleanup(); }
    };
    const cleanup = () => {
      if (io) io.disconnect();
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
    };
    if (typeof IntersectionObserver !== "undefined") {
      io = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) { setShown(true); cleanup(); } }, { rootMargin: LAZY_MARGIN + "px 0px" });
      io.observe(el);
    }
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    return cleanup;
  }, [shown]);
  return shown;
}

const OverlayContext = createContext(null);

export function Stage({ variant = "", grid = false, children, style, eager = false, minHeight = 220 }) {
  const ref = useRef(null);
  const shown = useNearViewport(ref, eager);
  const [on, setOn] = useState(false);
  const [annotated, setAnnotated] = useState(false);
  const ctx = useRef({ on: false, register: () => setAnnotated(true) });
  ctx.current.on = on;
  const overlay = shown && (grid || annotated);
  return (
    <div ref={ref} className={"stage " + variant} style={shown ? style : { ...style, minHeight }}>
      {overlay && (
        <button type="button" className="stage-overlay-toggle" aria-pressed={on} onClick={() => setOn(!on)}>Grid</button>
      )}
      {shown && (
        <OverlayContext.Provider value={{ ...ctx.current, on }}>
          {grid && on && <div className="gridover" />}
          {children}
        </OverlayContext.Provider>
      )}
    </div>
  );
}

// A dimension badge. It exists only while its stage's overlay is on, so it never sits in the DOM for the check.
export function AnnoTag({ style, children }) {
  const overlay = useContext(OverlayContext);
  useEffect(() => { if (overlay) overlay.register(); }, []);
  return overlay && overlay.on ? <span className="anno-tag" style={style}>{children}</span> : null;
}

export function Cluster({ label, children }) {
  return (
    <div className="cluster">
      {label && <span className="lbl">{label}</span>}
      <div className="row">{children}</div>
    </div>
  );
}

// Sources keep the full "--tasty-bg-app" so they stay searchable. The tables draw the bare name.
const bareTok = (s) => (typeof s === "string" ? s.replace(/--tasty-/g, "") : s);
const textOf = (node) => {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && node.props) return textOf(node.props.children);
  return "";
};
const bareNode = (node) => {
  if (typeof node === "string") return bareTok(node);
  if (Array.isArray(node)) return node.map((n, i) => <React.Fragment key={i}>{bareNode(n)}</React.Fragment>);
  if (node && node.props && node.props.children !== undefined) return React.cloneElement(node, undefined, bareNode(node.props.children));
  return node;
};

function ResolvedLength({ name }) {
  const [v, set] = useState("");
  useEffect(() => { set(resolveToken(name)); }, [name]);
  return <>{v}</>;
}

// specs: [[label, valueNode], ...]   tokens: [{ tok, use, color }]
export function Meta({ specs, tokens }) {
  return (
    <>
      {specs && (
        <div className="spec-table" tabIndex={0}>
          <table>
            <thead><tr><th>Layout</th><th>Token</th><th>Value</th></tr></thead>
            <tbody>
              {specs.map(([k, v], i) => {
                const m = /--tasty-([\w-]+)/.exec(textOf(v));
                return (
                  <tr key={i}>
                    <td>{bareNode(k)}</td>
                    <td className="m">{m ? bareNode(v) : null}</td>
                    <td className="m">{m ? <ResolvedLength name={m[1]} /> : bareNode(v)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {tokens && (
        <div className="spec-table" tabIndex={0}>
          <table>
            <thead><tr><th>Token</th><th>Use</th></tr></thead>
            <tbody>
              {tokens.map((t, i) => (
                <tr key={i}>
                  <td className="m" title={t.tok}>{t.color && <span className="sw" style={{ background: t.color }} />}{bareTok(t.tok)}</td>
                  <td className="d">{bareNode(t.use)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

export function Note({ children }) { return <p className="note">{children}</p>; }
export function Do({ children }) { return <p className="do">{children}</p>; }
export function Dont({ children }) { return <p className="dont">{children}</p>; }

// Gallery-only inline glyph (2px stroke). Product UI uses <Icon name>.
export function GIcon({ d, size = 16, fill }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill={fill || "none"}
      stroke={fill ? "none" : "currentColor"} strokeWidth="2" strokeLinecap="round"
      strokeLinejoin="round" style={{ display: "block" }}>{d}</svg>
  );
}
