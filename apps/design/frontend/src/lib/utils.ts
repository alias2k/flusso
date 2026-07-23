import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/// Merge class names, resolving Tailwind conflicts (the last wins). The standard
/// shadcn helper — `cn("p-2", isOn && "bg-accent", className)`.
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
