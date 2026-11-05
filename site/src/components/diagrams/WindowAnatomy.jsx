import React from "react";
import { TitleBar, Sidebar } from "../../../vendor/ui_kits/terminal/chrome.jsx";
import { TabStrip, TerminalPane, StatusBar, Prompt } from "../../../vendor/ui_kits/terminal/work.jsx";

/** Label the vendored kit regions. Wrappers keep label styles separate
 * from the kit components' inline styles. */
const noop = () => {};

const workspaces = [
  { id: "prod", name: "agents-prod", status: "running", notif: 0 },
  { id: "infra", name: "infra", status: "idle", notif: 0 },
  { id: "scratch", name: "scratch", status: "idle", notif: 0 },
];

const tabs = [
  { id: "t1", title: "zsh", kind: "terminal", owner: "user", activity: "idle" },
  { id: "t2", title: "README.md", kind: "markdown", owner: "user", activity: "idle" },
];

const session = {
  id: "s_01",
  lines: [<Prompt branch="main" />, <><span style={{ color: "var(--tasty-accent-agent-raised)" }}>❯</span> tasty list tree</>],
};

/** `grow` marks the one region that should absorb the leftover height. */
const Part = ({ label, grow, children }) => (
  <div className="anat" data-label={label} data-grow={grow ? "" : undefined}>{children}</div>
);

export function WindowAnatomy({ labels }) {
  return (
    <div className="anat-window">
      <div className="shell">
        <Part label={labels.titlebar}><TitleBar workspace="agents-prod" /></Part>
        <div className="shell__body">
          <Part label={labels.sidebar}>
            <div className="shell__side">
              <Sidebar workspaces={workspaces} activeWs="prod" workspacesHeading={labels.workspaces}
                onWs={noop} onSettings={noop} onPlugins={noop} onTools={noop} onToggle={noop} />
            </div>
          </Part>
          <div className="shell__work">
            <Part label={labels.tabstrip}>
              <TabStrip tabs={tabs} active="t1" onSelect={noop} onClose={noop} onNew={noop} onSearch={noop} />
            </Part>
            <Part label={labels.work} grow>
              <div className="shell__panes">
                <div className="shell__pane"><TerminalPane session={session} focused /></div>
              </div>
            </Part>
            <Part label={labels.statusbar}>
              <div className="shell__status">
                <StatusBar surfaceId="s_01" theme="Mocha" onTheme={noop} onPalette={noop} />
              </div>
            </Part>
          </div>
        </div>
      </div>
    </div>
  );
}
