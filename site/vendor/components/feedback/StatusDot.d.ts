import * as React from "react";

export interface StatusDotProps extends React.HTMLAttributes<HTMLSpanElement> {
  /** Host ground. "raised" and "active" read the matrix cell for that ground. Leave out on a neutral ground. */
  ground?: "raised" | "active";
  /** State color. `needs-input` / `completion` are attention kinds, not execution states. */
  status?: "running" | "idle" | "agent" | "waiting" | "error" | "needs-input" | "completion";
  /** Animate an expanding pulse ring (live/running). */
  pulse?: boolean;
  /** Optional text label after the dot. */
  label?: React.ReactNode;
}

/**
 * Small state indicator for surfaces, panes, and plugins, with an
 * optional pulse and label.
 */
export function StatusDot(props: StatusDotProps): JSX.Element;
