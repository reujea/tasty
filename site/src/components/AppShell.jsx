import React from "react";
import { createPortal } from "react-dom";
import { Toast } from "../ds/feedback/Toast.jsx";
import { TitleBar, Sidebar } from "../kit/chrome.jsx";
import { CommandPalette } from "../kit/overlays/command_palette.jsx";
import { PluginsWindow, pluginAttentionCount } from "../kit/overlays/plugins_window.jsx";
import { PortsWindow } from "../kit/overlays/port_scanner.jsx";
import { RemoteTool } from "../kit/overlays/remote_tool.jsx";
import { SettingsWindow } from "../kit/overlays/settings_window.jsx";
import { ToolsMenu } from "../kit/overlays/tools_menu.jsx";
import { TabStrip, TerminalPane, MarkdownSurface, StatusBar, Prompt } from "../kit/work.jsx";

/**
 * The app itself, composed from the shipped UI kit, as the landing page's
 * illustration — and it works: tabs switch, close and open, workspaces switch,
 * the sidebar collapses, and the sidebar's buttons open the real windows.
 *
 * This is the kit's own composition (`kit/app.jsx`) with two things changed for
 * a page rather than a preview:
 *
 *   - Theme belongs to the page, not to this component. The status bar and the
 *     Settings window drive the site's own toggle, so the whole page follows.
 *   - The windows (Settings 1100x700, Plugins 820x540) are larger than the
 *     frame the hero gives the shell, and the tools menu positions itself
 *     against the viewport. So overlays render into a fixed full-page portal
 *     instead of inside the frame, at their real size.
 */
const c = {
  green: "var(--tasty-color-green)", blue: "var(--tasty-color-blue)", mauve: "var(--tasty-color-mauve)",
  yellow: "var(--tasty-color-yellow)", dim: "var(--tasty-color-neutral-700)", teal: "var(--tasty-color-teal)",
};
const t = (color) => (txt) => <span style={{ color }}>{txt}</span>;

// The two panes are the product's whole claim in one frame: an agent building
// on the left while the operator keeps working on the right, in one workspace.
const agentSession = (task) => ({
  id: "s_01HX",
  lines: [
    <><span style={{ color: c.mauve }}>❯</span> claude "{task}"</>,
    t(c.dim)("   reading CHANGELOG.md, Cargo.toml …"),
    <><span style={{ color: c.blue }}>tasty agent.task</span> {t(c.dim)("id=t_build")} cargo build --release</>,
    t(c.dim)("   Compiling tasty-agent v0.7.1"),
    t(c.dim)("   Compiling tasty-host-plugin v0.7.1"),
    <><span style={{ color: c.green }}>    Finished</span> <span style={{ color: c.yellow }}>release</span> [optimized] in 42.18s</>,
    <><span style={{ color: c.blue }}>agent.task</span> <span style={{ color: c.green }}>done</span> id=t_build exit=0</>,
  ],
});

// Demonstration input only; the embedded terminal does not execute commands.
const mixedCommand = "tasty split --level surface --target-surface 4 --type markdown --file notes/split-layout.md";
const mixedSession = {
  id: "4",
  plainLines: ["~/work/tasty", mixedCommand],
  lines: [t(c.dim)("~/work/tasty"), <><span style={{ color: c.mauve }}>❯</span> {mixedCommand}</>],
};

const userSession = {
  id: "s_02JK",
  // plain-text mirror of `lines` — what the search bar matches against
  plainLines: [
    "~/tasty main via v1.84",
    "❯ tasty list surfaces | jq '.[].type'",
    '"Terminal"', '"Explorer"', '"Terminal"',
    "~/tasty main via v1.84",
    "❯ tasty read since-mark --surface 12",
  ],
  lines: [
    <Prompt branch="main" />,
    <><span style={{ color: c.mauve }}>❯</span> tasty list surfaces | jq '.[].type'</>,
    t(c.green)('"Terminal"'),
    t(c.green)('"Explorer"'),
    t(c.green)('"Terminal"'),
    <Prompt branch="main" />,
    <><span style={{ color: c.mauve }}>❯</span> tasty read since-mark --surface 12</>,
  ],
};

// Every workspace field combination the sidebar knows how to draw, so the
// illustration shows the real range rather than one happy row.
const workspaces = [
  { id: "prod", name: "agents-prod", status: "running", notif: 0 },
  { id: "infra", name: "infra", subtitle: "mirror → prod-web", status: "idle", notif: 0, mirror: true, mirrorTarget: "prod-web" },
  { id: "api", name: "api-gateway", description: "deploy to staging on every push to main", status: "agent", notif: 2, attached: true },
  { id: "scratch", name: "scratch", status: "idle", notif: 0 },
];

const INITIAL_TABS = [
  { id: "t1", title: "claude · release", kind: "terminal", owner: "agent", activity: "running", attached: true },
  { id: "t2", title: "README.md", kind: "markdown", owner: "user", activity: "idle", notif: true },
  { id: "t3", title: "terminal + docs", kind: "split", owner: "user", activity: "idle" },
];

/* Theme is the page's, not this component's. `site.js` owns applying and
   persisting it; this only reads the attribute it sets and asks it to set
   another. Falls back to a local toggle if that script has not run. */
function readTheme() {
  if (typeof document === "undefined") return "mocha";
  if (window.tastyTheme) return window.tastyTheme.get();
  return document.documentElement.getAttribute("data-theme") === "latte" ? "latte" : "mocha";
}

function usePageTheme() {
  const [theme, set] = React.useState("mocha");
  React.useEffect(() => {
    const read = () => set(readTheme());
    read();
    const mo = new MutationObserver(read);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => mo.disconnect();
  }, []);
  const apply = React.useCallback((name) => {
    if (window.tastyTheme) window.tastyTheme.set(name);
    else document.documentElement.toggleAttribute("data-theme", name === "latte");
    set(name);
  }, []);
  return [theme, apply];
}

/* Overlays go to the end of <body> so they get the whole viewport: the windows
   are bigger than the hero's frame, and ToolsMenu anchors to viewport
   coordinates. Renders nothing until mounted, so the static HTML has no
   overlay markup and hydration has nothing to mismatch. */
function OverlayLayer({ open, lock, scale, children }) {
  const [host, setHost] = React.useState(null);
  React.useEffect(() => { setHost(document.body); }, []);
  React.useEffect(() => {
    if (!lock) return;
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => { document.body.style.overflow = prev; };
  }, [lock]);
  if (!host || !open) return null;
  return createPortal(
    <div className="shell-overlays" style={scale}>{children}</div>,
    host,
  );
}

export function AppShell({ heading = "Workspaces", task = "run tests", lang = "en" }) {
  const ko = lang === "ko";
  const [theme, setTheme] = usePageTheme();
  const [overlay, setOverlay] = React.useState(null);
  const [toolsAnchor, setToolsAnchor] = React.useState(null);
  const [searchOpen, setSearchOpen] = React.useState(false);
  const [activeWs, setActiveWs] = React.useState("prod");
  const [activeTab, setActiveTab] = React.useState("t3");
  const [tabs, setTabs] = React.useState(INITIAL_TABS);
  const [collapsed, setCollapsed] = React.useState(false);
  const [uiScale, setUiScale] = React.useState("md");
  const [toast, setToast] = React.useState(null);
  const seq = React.useRef(4);
  const toastTimer = React.useRef(0);

  // The kit scales its chrome off one variable. The app sets it on :root; here
  // it stays on the shell and the overlay layer, so the page around it is not
  // resized by a control inside the picture.
  const scale = { "--tasty-ui-scale": `var(--tasty-ui-scale-${uiScale})` };

  const anyOverlay = overlay !== null || toolsAnchor !== null;
  React.useEffect(() => {
    if (!anyOverlay) return;
    const h = (e) => { if (e.key === "Escape") { setOverlay(null); setToolsAnchor(null); } };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [anyOverlay]);

  // Search is scoped to the focused surface — close it when the tab changes.
  React.useEffect(() => { setSearchOpen(false); }, [activeTab]);

  const flash = (msg) => {
    setToast(msg);
    clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 1800);
  };

  const newTab = () => {
    const id = "t" + seq.current++;
    setTabs((prev) => [...prev, { id, title: "terminal " + (seq.current - 1), kind: "terminal", owner: "user", activity: "idle" }]);
    setActiveTab(id);
  };
  const closeTab = (id) => {
    setTabs((prev) => {
      const next = prev.filter((x) => x.id !== id);
      if (id === activeTab && next.length) setActiveTab(next[0].id);
      return next;
    });
  };
  const runCmd = (label) => {
    setOverlay(null);
    if (label.startsWith("Toggle Theme")) setTheme(theme === "latte" ? "mocha" : "latte");
    else if (label === "Settings") setOverlay("settings");
    else if (label.startsWith("Listening ports")) setOverlay("ports");
    else if (label === "New Terminal") newTab();
    else flash(label);
  };
  const runTool = (id) => {
    setToolsAnchor(null);
    if (id === "palette") setOverlay("palette");
    else if (id === "ports") setOverlay("ports");
    else if (id === "remote") setOverlay("remote");
    else if (id === "presets") flash("Layout Presets");
    else if (id === "clipboard") flash("Clipboard History");
    else if (id === "git") flash("git-helper: open panel");
  };

  const tab = tabs.find((x) => x.id === activeTab);
  const ws = workspaces.find((w) => w.id === activeWs);

  return (
    <div className="shell" style={scale}>
      <TitleBar workspace={ws ? ws.name : "tasty"} />
      <div className="shell__body">
        {/* Wrapped because the kit styles its own root inline, and an inline
            style beats any class rule the page could write to drop it. */}
        <div className="shell__side">
          <Sidebar workspaces={workspaces} activeWs={activeWs} workspacesHeading={heading}
            collapsed={collapsed} onToggle={() => setCollapsed((v) => !v)}
            pluginAlert={pluginAttentionCount || 0}
            onWs={setActiveWs} onSettings={() => setOverlay("settings")}
            onPlugins={() => setOverlay("plugins")}
            onTools={(rect) => setToolsAnchor((a) => (a ? null : rect))} />
        </div>
        <div className="shell__work">
          <TabStrip tabs={tabs} active={activeTab} onSelect={setActiveTab} onClose={closeTab}
            onNew={newTab} onSearch={() => setSearchOpen((v) => !v)} />
          <div className={`shell__panes${tab?.kind === "split" ? " shell__panes--mixed" : ""}`}>
            {tab && tab.kind === "markdown" ? (
              <div className="shell__pane"><MarkdownSurface focused /></div>
            ) : tab?.kind === "split" ? (
              <>
                <div className="shell__pane">
                  <TerminalPane session={mixedSession} focused
                    searchOpen={searchOpen} onSearchClose={() => setSearchOpen(false)} />
                </div>
                <div className="shell__pane">
                  <MarkdownSurface path="~/work/tasty/notes/split-layout.md">
                    <h2 style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-prose-h1)", margin: "0 0 var(--tasty-space-sm)" }}>
                      {ko ? "실행은 왼쪽, 문서는 오른쪽" : "Run it left, read it right"}
                    </h2>
                    <p style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>
                      {ko ? "한 탭 안에 터미널과 문서를 나란히 두세요. 명령을 실행하면서 필요한 설명을 바로 확인할 수 있습니다." : "Keep a terminal and a document side by side in one tab. Run a command while keeping its instructions in view."}
                    </p>
                    <h3 style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-accent-primary)" }}>
                      {ko ? "같이 두고, 함께 전환하세요" : "Keep them together"}
                    </h3>
                    <p style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>
                      {ko ? "다른 탭을 선택하면 이 배치가 함께 전환됩니다. 터미널을 선택한 상태에서는 검색과 상태바도 그 터미널을 기준으로 표시합니다." : "Switch tabs to change the whole layout. With the terminal focused, search and the status bar refer to that terminal."}
                    </p>
                  </MarkdownSurface>
                </div>
              </>
            ) : (
              <>
                <div className="shell__pane shell__pane--agent">
                  <TerminalPane session={agentSession(task)} focused={false} />
                </div>
                <div className="shell__pane">
                  <TerminalPane session={userSession} focused
                    searchOpen={searchOpen} onSearchClose={() => setSearchOpen(false)} />
                </div>
              </>
            )}
          </div>
          <div className="shell__status">
            <StatusBar surfaceId={tab?.kind === "markdown" ? "3" : tab?.kind === "split" ? "4" : "2"} theme={theme}
              onTheme={() => setTheme(theme === "latte" ? "mocha" : "latte")}
              onPalette={() => setOverlay("palette")} />
          </div>
        </div>
      </div>

      <OverlayLayer open={anyOverlay || toast !== null} lock={anyOverlay} scale={scale}>
        {toolsAnchor && <ToolsMenu anchor={toolsAnchor} onClose={() => setToolsAnchor(null)} onAction={runTool} />}
        {overlay === "palette" && <CommandPalette onClose={() => setOverlay(null)} onRun={runCmd} />}
        {overlay === "settings" && <SettingsWindow theme={theme} onTheme={setTheme}
          uiScale={uiScale} onUiScale={setUiScale} onClose={() => setOverlay(null)} />}
        {overlay === "ports" && <PortsWindow onClose={() => setOverlay(null)} onFlash={flash} />}
        {overlay === "remote" && <RemoteTool onClose={() => setOverlay(null)} onFlash={flash} />}
        {overlay === "plugins" && <PluginsWindow onClose={() => setOverlay(null)} onFlash={flash}
          onConfigure={() => setOverlay("settings")} />}
        {toast && <div className="shell-overlays__toast">{React.createElement(Toast, { variant: "info" }, toast)}</div>}
      </OverlayLayer>
    </div>
  );
}
