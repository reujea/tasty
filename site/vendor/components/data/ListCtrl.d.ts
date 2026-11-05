import * as React from "react";

export interface ListCtrlItem {
  /** Unique row identity. */
  id: React.Key;
  /** Primary row label. */
  label: React.ReactNode;
  /** Optional secondary line under the label (muted, caption size). */
  description?: React.ReactNode;
  /** Optional leading glyph (an <Icon/> node). */
  icon?: React.ReactNode;
  /** Optional trailing node before the chevron — e.g. an "Active" Tag/Badge. */
  trailing?: React.ReactNode;
  /** Non-selectable, dimmed row (also hides its chevron). */
  disabled?: boolean;
}

export interface ListCtrlProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, "onSelect"> {
  /** Rows, top to bottom. */
  items: ListCtrlItem[];
  /** Selected row id (renders --tasty-surface-active + accent left bar). */
  selectedId?: React.Key | null;
  /** The row the keyboard is on. Paints the selected ground without selecting it. */
  activeId?: React.Key | null;
  /** Row click handler; receives (id, item). */
  onSelect?: (id: React.Key, item: ListCtrlItem) => void;
  /** Show the trailing drill-in chevron. Default true. */
  chevron?: boolean;
  /** Hairline divider between rows. Default true. */
  divided?: boolean;
  /** Empty-state content when `items` is empty. */
  empty?: React.ReactNode;
}

/**
 * Full-width, row-selectable NAVIGATION list — a "pick one to drill into"
 * list (not a data grid; use Table for sortable multi-column data). Each row
 * is a primary label with optional description, leading icon, a trailing slot
 * (Tag/Badge), and a drill-in chevron. Pair with DrillDown for list → detail.
 */
export function ListCtrl(props: ListCtrlProps): JSX.Element;
