import * as React from "react";

export interface SwitchProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "type"> {
  /** Text label rendered next to the toggle. */
  label?: React.ReactNode;
  /** "raised" when the Switch sits on surface-raised or on-raised, so the off track holds 3:1. */
  ground?: "raised";
}

/**
 * Compact toggle for boolean settings. Accent-filled when on.
 */
export function Switch(props: SwitchProps): JSX.Element;
