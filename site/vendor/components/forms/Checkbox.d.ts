import * as React from "react";

export interface CheckboxProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "type"> {
  /** Host ground. "raised" and "active" read the matrix cell for that ground. Leave out on a neutral ground. */
  ground?: "raised" | "active";
  /** Text label rendered next to the box. */
  label?: React.ReactNode;
}

/**
 * 16px square checkbox with accent fill + checkmark when checked.
 */
export function Checkbox(props: CheckboxProps): JSX.Element;
