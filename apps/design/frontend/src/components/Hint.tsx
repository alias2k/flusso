import type { ReactNode } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/// A hover/focus hint on any control — the Radix-backed replacement for the old
/// `[data-tip]` CSS tooltips. Wraps the child in a ref-able span so `asChild`
/// works for any trigger (incl. our non-forwardRef Button and disabled
/// buttons, which still surface their hint). Keep an `aria-label` on the child
/// for the accessible name; this is the visible affordance.
export function Hint({
  label,
  side = "bottom",
  className,
  children,
}: {
  label: string;
  side?: "top" | "bottom" | "left" | "right";
  /// Forwarded to the tooltip content — e.g. a `max-w-*` so long text wraps.
  className?: string;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex">{children}</span>
      </TooltipTrigger>
      <TooltipContent side={side} className={className}>
        {label}
      </TooltipContent>
    </Tooltip>
  );
}
