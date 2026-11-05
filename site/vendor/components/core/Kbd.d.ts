import * as React from "react";

export interface KbdProps extends React.HTMLAttributes<HTMLSpanElement> {
  /** Host ground. "raised" and "active" read the matrix cell for that ground. Leave out on a neutral ground. */
  ground?: "raised" | "active";
  /** Keys as an array (["Ctrl","Shift","T"]) or a "+"-joined string ("Ctrl+T"). */
  keys: string[] | string;
}

/**
 * Renders a keyboard shortcut as individual keycaps joined by "+".
 * Used in settings, menus, and the command palette.
 */
export function Kbd(props: KbdProps): JSX.Element;
