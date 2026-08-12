/**
 * Shared admin field label.
 *
 * Wraps a `<Label>` with a consistent layout (label + optional required marker +
 * children) used across all admin capability packages. Extracted to admin-core
 * to eliminate 12+ duplicate definitions.
 */

import * as React from "react";
import { Label } from "@sdkwork/ui-pc-react";

export interface AdminFieldLabelProps {
  children: React.ReactNode;
  className?: string;
  htmlFor: string;
  label: string;
  required?: boolean;
  /** Optional one-line explanation rendered under the label to help operators
   *  understand the field's purpose and where the value comes from. */
  hint?: React.ReactNode;
}

export function AdminFieldLabel({ children, className, htmlFor, label, required, hint }: AdminFieldLabelProps) {
  return (
    <div className={["space-y-1.5", className].filter(Boolean).join(" ")}>
      <Label htmlFor={htmlFor}>
        {label}
        {required ? <span className="text-[var(--sdk-color-text-error)]">*</span> : null}
      </Label>
      {hint ? (
        <p className="text-xs leading-relaxed text-[var(--sdk-color-text-muted)]">{hint}</p>
      ) : null}
      {children}
    </div>
  );
}
