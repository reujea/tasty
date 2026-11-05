// Tasty Gallery: what each contract of tokens/ground-ink-rules.json draws in a frozen cell.
// A contract with a component renders that component. The rest are written with the contract's tokens (tokenCell).
// render(state, host) returns the content of one cell. parts/part-map.js says where each part is inside it.
import React from "react";
import { Button, IconButton, Icon, Badge, Tag, Kbd, Input, Select, MenuItem, AutoComplete, MultiSelect, SearchableSelect, Checkbox, Switch, Tab, TreeRow, ListCtrl, Table, Toast, Tooltip, HelpHint, StatusDot, Spinner, NotificationBadge, NotificationItem, PendingIndicator, CenterState, CodeArea, DrillDown } from "../../components/index.js";
import { NotificationSummary } from "../../ui_kits/terminal/notification_summary.jsx";
import { tokenCell } from "./contract-parts.jsx";

const groundOf = (host) => (host === "raised" ? "raised" : host === "active" ? "active" : "panel");
const off = (state) => (state === "disabled" ? { disabled: true } : {});
const noop = () => {};
const plus = <Icon name="plus" />;
const chipHost = (variant) => () => <Badge variant={variant}>3</Badge>;
const selected = (state) => state.startsWith("selected");

const button = (variant, ground) => (state, host) => (
  <Button variant={variant} ground={ground || groundOf(host)} leadingIcon={plus} {...off(state)}>Label</Button>
);
const inputCell = (ground) => (state) => {
  const common = { icon: <Icon name="search" />, ground, invalid: state === "invalid", readOnly: state === "readonly", onChange: noop };
  return (
    <>
      <Input {...common} data-part="value" value="Value" />
      <Input {...common} data-part="placeholder" value="" placeholder="Hint" />
    </>
  );
};
const selectCell = (ground) => () => (
  <>
    <Select ground={ground} data-part="value" options={["Value"]} />
    <Select ground={ground} data-part="placeholder" placeholder="Hint" options={["Value"]} />
  </>
);
const tabParts = (extra) => () => (
  <>
    <Tab label="Tab" icon={<Icon name="terminal" />} status="busy" {...extra} data-part="tab" />
    {!extra.active && <Tab label="Needs input" attention="needs-input" {...extra} />}
    {!extra.active && <Tab label="Done" attention="completion" {...extra} />}
  </>
);
const treeRow = (state) => (
  <>
    <TreeRow label="Folder" expandable open meta="3" selected={selected(state)} />
    <TreeRow label="File" icon={<Icon name="file" />} selected={selected(state)} />
  </>
);
const listItems = (state) => [
  { id: "a", label: "Label", description: "Description", icon: <Icon name="file" />, disabled: state === "disabled" },
  { id: "b", label: "Second", description: "Description", disabled: state === "disabled" },
];
const notificationItem = (state) => (
  <>
    {["error", "warning", "info"].map((level) => (
      <NotificationItem key={level} level={level} title="Title" body={"Body line\n".repeat(12)} source="Source" time="1m" unread
        selected={selected(state)} hover={state.includes("hover") || state.includes("press")} style={{ width: "var(--tasty-size-240)" }} />
    ))}
  </>
);
const statusDots = (state, host) => (
  <>
    <StatusDot data-part="idle" status="idle" ground={host} />
    <StatusDot data-part="success" status="running" ground={host} />
    <StatusDot data-part="warning" status="waiting" ground={host} />
    <StatusDot data-part="danger" status="error" ground={host} />
    <StatusDot data-part="agent" status="agent" ground={host} />
    <StatusDot data-part="completion" status="completion" ground={host} />
    <StatusDot data-part="attached-ring" status="idle" attached ground={host} />
  </>
);
const toasts = () => (
  <>
    {[["info", "info"], ["success", "check"], ["warning", "alertTriangle"], ["danger", "close"], ["agent", "agent"]].map(([variant, name]) => (
      <Toast key={variant} variant={variant} data-part={"accent." + variant} icon={<Icon name={name} />}>Toast</Toast>
    ))}
  </>
);

const CELLS = {
  "Button.secondary": button("secondary"),
  "Button.secondary (on raised)": button("secondary", "raised"),
  "Button.primary": button("primary"),
  "Button.agent": button("agent"),
  "Button.danger": button("danger"),
  "Button.ghost": button("ghost"),
  IconButton: (state, host) => <IconButton aria-label="Add" ground={groundOf(host)} {...off(state)}>{plus}</IconButton>,
  "Badge.default": chipHost("danger"),
  "Badge.danger": chipHost("danger"),
  "Badge.primary": chipHost("primary"),
  "Badge.warning": chipHost("warning"),
  "Badge.agent": chipHost("agent"),
  "Badge.success": chipHost("success"),
  "Badge.neutral": chipHost("neutral"),
  Tag: (state, host) => (
    <>
      <Tag data-part="label" ground={host} disabled={state === "disabled"}>Tag</Tag>
      {state === "rest" && ["success", "warning", "attention", "danger", "info", "attached"].map((v) => <Tag key={v} variant={v} data-part={"label." + v}>{v}</Tag>)}
    </>
  ),
  Kbd: (state, host) => <Kbd keys={["Ctrl", "K"]} ground={host} />,
  Input: inputCell("panel"),
  "Input (on raised)": inputCell("raised"),
  "Select trigger": selectCell("panel"),
  "Select trigger (on raised)": selectCell("raised"),
  MenuItem: (state) => {
    const disabled = state === "disabled";
    const active = state === "keyboard-active";
    return (
      <>
        <MenuItem data-part="item" label="Item" icon={plus} shortcut="Ctrl+K" active={active} disabled={disabled} />
        <MenuItem data-part="danger" label="Delete" danger active={active} disabled={disabled} />
        <MenuItem data-part="chosen" label="Chosen" selected active={active} disabled={disabled} />
      </>
    );
  },
  "AutoComplete row": (state) => (
    <AutoComplete open query="fo" items={["foo", "food"]} rowIcon={<Icon name="file" />}
      activeIndex={state === "keyboard-active" ? 0 : -1} hoverIndex={state === "hover" ? 0 : null} />
  ),
  "MultiSelect row": (state) => (
    <MultiSelect open allToggle options={["One", "Two"]} defaultValue={[]}
      activeIndex={state === "keyboard-active" ? 0 : -1} hoverIndex={state === "hover" ? 0 : null} />
  ),
  "SearchableSelect status row": () => <SearchableSelect open loading options={[]} />,
  "Checkbox.off": (state, host) => <Checkbox aria-label="Off" ground={host} {...off(state)} checked={false} onChange={noop} />,
  "Checkbox.on": (state, host) => <Checkbox aria-label="On" ground={host} {...off(state)} checked onChange={noop} />,
  "Switch.off": (state, host) => <Switch aria-label="Off" ground={host === "raised" ? "raised" : undefined} checked={false} onChange={noop} />,
  "Switch.off (on raised)": () => <Switch aria-label="Off" ground="raised" checked={false} onChange={noop} />,
  "Switch.on": (state, host) => <Switch aria-label="On" ground={host === "raised" ? "raised" : undefined} checked onChange={noop} />,
  "Tab.inactive": tabParts({}),
  "Tab.hover": tabParts({ "data-force": "hover" }),
  "Tab.active": tabParts({ active: true }),
  TreeRow: treeRow,
  "ListCtrl row": (state) => <ListCtrl items={listItems(state)} selectedId={selected(state) ? "a" : null} activeId={state === "keyboard-active" ? "a" : null} onSelect={noop} />,
  "Table row": (state) => (
    <Table columns={[{ key: "a", header: "Name" }]} rows={[{ a: "Cell" }, { a: "Second" }]} rowKey="a" selectedKey={selected(state) ? "Cell" : null} onRowClick={noop} />
  ),
  "Table header": () => <Table columns={[{ key: "a", header: "Name" }]} rows={[]} empty="" />,
  Toast: toasts,
  Tooltip: () => <Tooltip open content="Tooltip"><span /></Tooltip>,
  HelpHint: () => <HelpHint />,
  StatusDot: statusDots,
  Spinner: () => <Spinner paused />,
  "NotificationBadge.error": () => <NotificationBadge level="error" count={3} data-part="count" />,
  "NotificationBadge.warning": () => <NotificationBadge level="warning" count={3} data-part="count" />,
  "NotificationBadge.info": () => <NotificationBadge level="info" count={3} data-part="count" />,
  NotificationItem: notificationItem,
  NotificationSummary: (state) => <NotificationSummary counts={{ info: 1 }} latest={{ level: "info", title: "Title", time: "1m" }} hover={state !== "rest"} />,
  PendingIndicator: () => <PendingIndicator count={2} time="0:12" reducedMotion />,
  CenterState: () => (
    <>
      <CenterState natural data-part="plain" title="Title" sub="Line" />
      <CenterState natural data-part="failed" variant="error" title="Title" sub="Line" />
    </>
  ),
  CodeArea: () => (
    <>
      <CodeArea minRows={2} value="code" onChange={noop} />
      <CodeArea minRows={2} errorLine={1} value="code" onChange={noop} />
    </>
  ),
  "CodeArea gutter": () => <CodeArea minRows={2} value="code" onChange={noop} />,
  "DrillDown back bar": () => <DrillDown view="detail" title="Title" detail={null} />,
};

// A contract with no component here is written with tokens.
export const cellFor = (contract) => CELLS[contract] || tokenCell(contract);

// Rows that react to a pseudo-class, besides the cell's direct children.
export const FORCE_ALSO = {
  "ListCtrl row": ".tasty-listctrl__row",
  "Table row": ".tasty-table__row",
  "Select trigger": "select",
  "Select trigger (on raised)": "select",
  "Checkbox.off": "input",
  "Checkbox.on": "input",
  "Switch.off": "input",
  "Switch.off (on raised)": "input",
  "Switch.on": "input",
};
