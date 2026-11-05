import * as React from "react";

/**
 * @startingPoint section="Core" subtitle="Buttons — primary, secondary, ghost, danger, agent" viewport="700x300"
 */
export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** Visual role. primary/agent/danger are filled; secondary is outlined; ghost is bare. */
  variant?: "primary" | "secondary" | "ghost" | "danger" | "agent";
  /** Control height. md = 28px (default), sm = 24px, lg = 32px. */
  size?: "sm" | "md" | "lg";
  /** Stretch to fill the container width. */
  block?: boolean;
  /** Icon element rendered before the label. */
  leadingIcon?: React.ReactNode;
  /** Icon element rendered after the label. */
  trailingIcon?: React.ReactNode;
  /** 2026-10-21 — the ground the button sits on. "raised" (surface-raised: add cards, menus) swaps the ghost label to
   *  --tasty-button-ghost-raised-fg so it keeps 4.5:1 in Latte. Other variants are unchanged.
   *  2026-11-05 — "active" (a selected row's surface-active fill) reads text-on-active the same way. */
  ground?: "panel" | "raised" | "active";
  children?: React.ReactNode;
}

/**
 * The primary text button. 28px tall, 4px radius, 1px border, with
 * 8%/12% hover/active overlays derived from the active theme.
 */
export function Button(props: ButtonProps): JSX.Element;
