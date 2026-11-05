import * as React from "react";

export interface IconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** md = 28px (default), sm = 24px to match the tab strip. */
  size?: "sm" | "md";
  /** ghost (default, bare) or solid (raised surface + border). */
  variant?: "ghost" | "solid";
  /** Show the persistent active/selected state (accent color). */
  active?: boolean;
  /** Host ground. "active" reads text-on-active on a selected row. */
  ground?: "panel" | "active";
  /** The icon element — normally `<Icon name="…" />` from components/core/Icon. */
  children?: React.ReactNode;
}

/**
 * Square, icon-only button for toolbars, tab-close, and sidebar actions.
 * Same height as Button; ghost by default with 8%/12% overlays.
 */
export function IconButton(props: IconButtonProps): JSX.Element;
