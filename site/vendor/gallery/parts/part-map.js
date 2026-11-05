// Where a contract's part lives inside a frozen cell, for the rendered check. Default: [data-part="<part>"].
// sel: the element; ":cell" is the cell. prop: the computed property to read, when it is not implied by the role.
//   "svg" the first shape's stroke or fill, "shadow" the first color of box-shadow.
const button = { label: { sel: ".tasty-btn > span" }, icon: { sel: ".tasty-btn" , prop: "svg" }, edge: { sel: ".tasty-btn", prop: "borderTopColor" }, ring: { sel: ".tasty-btn" } };
const count = { count: { sel: ".tasty-badge" } };
const input = {
  value: { sel: '[data-part="value"]' },
  placeholder: { sel: '[data-part="placeholder"]', pseudo: "::placeholder" },
  icon: { sel: ".tasty-input__icon", prop: "svg" },
  edge: { sel: ".tasty-input", prop: "borderTopColor" },
  "edge.invalid": { sel: ".tasty-input--invalid", prop: "borderTopColor" },
  "value.readonly": { sel: '[data-part="value"]' },
  ring: { sel: ".tasty-input", prop: "shadow" },
};
const select = {
  value: { sel: '.tasty-select:not(.tasty-select--empty) select' },
  placeholder: { sel: ".tasty-select--empty select" },
  chevron: { sel: ".tasty-select__chevron", prop: "svg" },
  edge: { sel: "select", prop: "borderTopColor" },
  ring: { sel: "select", prop: "shadow" },
};
const check = { edge: { sel: ".tasty-check__box" }, check: { sel: ".tasty-check__box", prop: "svg" }, ring: { sel: ".tasty-check__box" } };
const sw = { thumb: { sel: ".tasty-switch__thumb", prop: "backgroundColor" }, ring: { sel: ".tasty-switch__track" } };
const tab = {
  label: { sel: '[data-part="tab"] .tasty-tab__label' },
  "label.needs-input": { sel: ".tasty-tab--needs-input .tasty-tab__label" },
  "label.completion": { sel: ".tasty-tab--completion .tasty-tab__label" },
  icon: { sel: '[data-part="tab"] .tasty-tab__icon', prop: "svg" },
  close: { sel: '[data-part="tab"] .tasty-tab__close', prop: "svg" },
  dot: { sel: ".tasty-tab__dot--busy", prop: "backgroundColor" },
  separator: { sel: '[data-part="tab"]', prop: "borderRightColor" },
  indicator: { sel: ".tasty-tab--active", pseudo: "::before", prop: "backgroundColor" },
};
const row = (cell, border) => ({ cell: { sel: cell }, border: { sel: border, prop: "borderBottomColor" } });
export const PART_MAP = {
  "Button.secondary": button, "Button.secondary (on raised)": button, "Button.primary": button, "Button.agent": button, "Button.danger": button, "Button.ghost": button,
  IconButton: { glyph: { sel: ".tasty-iconbtn", prop: "svg" }, ring: { sel: ".tasty-iconbtn" } },
  "Badge.default": count, "Badge.danger": count, "Badge.primary": count, "Badge.warning": count, "Badge.agent": count, "Badge.success": count, "Badge.neutral": count,
  "Title bar": { button: { prop: "svg", sel: '[data-part="button"]' } }, "Title bar (inactive)": { button: { prop: "svg", sel: '[data-part="button"]' } },
  "Window close button": { glyph: { prop: "svg", sel: '[data-part="glyph"]' } },
  Tag: { edge: { sel: '[data-part="label"]' } },
  Kbd: { cap: { sel: "kbd" }, edge: { sel: "kbd" }, plus: { sel: ".tasty-kbd__plus" } },
  Input: input, "Input (on raised)": input,
  "Select trigger": select, "Select trigger (on raised)": select,
  MenuItem: {
    label: { sel: '[data-part="item"] .tasty-menuitem__label' },
    "label.danger": { sel: '[data-part="danger"] .tasty-menuitem__label' },
    icon: { sel: '[data-part="item"] .tasty-menuitem__icon', prop: "svg" },
    shortcut: { sel: '[data-part="item"] .tasty-menuitem__shortcut' },
    check: { sel: ".tasty-menuitem__check", prop: "svg" },
  },
  "AutoComplete row": { label: { sel: ".tasty-ac__row-label" }, match: { sel: ".tasty-ac__mark" }, icon: { sel: ".tasty-ac__row-icon", prop: "svg" } },
  "MultiSelect row": { label: { sel: ".tasty-mselect__row .tasty-check__label" }, bulk: { sel: ".tasty-mselect__all" } },
  "SearchableSelect status row": { status: { sel: ".tasty-sselect__status", prop: "color" } },
  "Checkbox.off": check, "Checkbox.on": check,
  "Switch.off": sw, "Switch.off (on raised)": sw, "Switch.on": sw,
  "Tab.inactive": tab, "Tab.hover": tab, "Tab.active": tab,
  TreeRow: {
    label: { sel: ".tasty-treerow__label" }, chevron: { sel: ".tasty-treerow__chevron", prop: "svg" }, meta: { sel: ".tasty-treerow__meta" },
    icon: { sel: ".tasty-treerow__icon", prop: "svg" }, "icon.selected": { sel: ".tasty-treerow--selected .tasty-treerow__icon", prop: "svg" },
  },
  "ListCtrl row": {
    label: { sel: ".tasty-listctrl__label" }, desc: { sel: ".tasty-listctrl__desc" }, icon: { sel: ".tasty-listctrl__icon", prop: "svg" },
    chevron: { sel: ".tasty-listctrl__chev", prop: "svg" }, divider: { sel: ".tasty-listctrl__row", prop: "borderBottomColor" },
    bar: { sel: ".tasty-listctrl__row.is-selected", prop: "shadow" },
  },
  "Table row": row(".tasty-table__row td", ".tasty-table__row td"),
  "Table header": { label: { sel: ".tasty-table__head th" } },
  Toast: {
    text: { sel: ".tasty-toast__msg" }, edge: { sel: ".tasty-toast", prop: "borderTopColor" },
    "accent.info": { sel: '[data-part="accent.info"] .tasty-toast__icon', prop: "svg" },
    "accent.success": { sel: '[data-part="accent.success"] .tasty-toast__icon', prop: "svg" },
    "accent.warning": { sel: '[data-part="accent.warning"] .tasty-toast__icon', prop: "svg" },
    "accent.danger": { sel: '[data-part="accent.danger"] .tasty-toast__icon', prop: "svg" },
    "accent.agent": { sel: '[data-part="accent.agent"] .tasty-toast__icon', prop: "svg" },
  },
  Tooltip: { text: { sel: ".tasty-tooltip__bubble" }, edge: { sel: ".tasty-tooltip__bubble" } },
  HelpHint: { glyph: { sel: ".tasty-help-hint__glyph", prop: "svg" } },
  StatusDot: {
    idle: { sel: '[data-part="idle"] .tasty-statusdot__dot', prop: "backgroundColor" },
    success: { sel: '[data-part="success"] .tasty-statusdot__dot', prop: "backgroundColor" },
    warning: { sel: '[data-part="warning"] .tasty-statusdot__dot', prop: "backgroundColor" },
    danger: { sel: '[data-part="danger"] .tasty-statusdot__dot', prop: "backgroundColor" },
    agent: { sel: '[data-part="agent"] .tasty-statusdot__dot', prop: "backgroundColor" },
    completion: { sel: '[data-part="completion"] .tasty-statusdot__dot', prop: "backgroundColor" },
    "attached-ring": { sel: '[data-part="attached-ring"] .tasty-statusdot__dot', prop: "outlineColor" },
  },
  Spinner: { indicator: { sel: ".tasty-spinner__arc", prop: "stroke" } },
  NotificationItem: {
    "glyph.error": { sel: '[aria-label*="rror:"] [data-part="glyph"]', prop: "svg" },
    "glyph.warning": { sel: '[aria-label*="arning:"] [data-part="glyph"]', prop: "svg" },
    "glyph.info": { sel: '[aria-label*="nfo:"] [data-part="glyph"]', prop: "svg" },
    unread: { sel: '[data-part="unread"]', prop: "backgroundColor" },
    more: { sel: '[data-part="more"]' },
    "more.selected": { sel: '[data-part="more"]' },
  },
  CenterState: {
    glyph: { sel: '[data-part="plain"] .tasty-cstate__glyph', prop: "svg" },
    error: { sel: '[data-part="failed"] .tasty-cstate__glyph', prop: "svg" },
    title: { sel: '[data-part="plain"] .tasty-cstate__title' },
    sub: { sel: '[data-part="plain"] .tasty-cstate__sub' },
  },
  CodeArea: { code: { sel: "textarea" }, error: { sel: ".tasty-codearea__gutter .is-error" }, edge: { sel: ".tasty-codearea:not(.tasty-codearea--invalid)", prop: "borderTopColor" } },
  "CodeArea gutter": { number: { sel: ".tasty-codearea__gutter span" } },
  "DrillDown back bar": { border: { sel: ".tasty-drilldown__backbar", prop: "borderBottomColor" }, title: { sel: ".tasty-drilldown__title" } },
};
