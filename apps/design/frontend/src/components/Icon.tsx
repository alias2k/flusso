// A tiny inline-SVG icon set (lucide-style strokes) so the UI doesn't rely on
// emoji, which render inconsistently across platforms. No dependency.

import type { ReactElement } from "react";

type IconName =
  | "menu"
  | "close"
  | "map"
  | "plus"
  | "chevron"
  | "flow"
  | "undo"
  | "redo"
  | "theme"
  | "copy"
  | "play"
  | "tidy";

const paths: Record<IconName, ReactElement> = {
  menu: (
    <>
      <line x1="3" y1="6" x2="17" y2="6" />
      <line x1="3" y1="10" x2="17" y2="10" />
      <line x1="3" y1="14" x2="17" y2="14" />
    </>
  ),
  close: (
    <>
      <line x1="5" y1="5" x2="15" y2="15" />
      <line x1="15" y1="5" x2="5" y2="15" />
    </>
  ),
  map: (
    <>
      <polygon points="2 5 7 3 13 5 18 3 18 15 13 17 7 15 2 17" />
      <line x1="7" y1="3" x2="7" y2="15" />
      <line x1="13" y1="5" x2="13" y2="17" />
    </>
  ),
  plus: (
    <>
      <line x1="10" y1="4" x2="10" y2="16" />
      <line x1="4" y1="10" x2="16" y2="10" />
    </>
  ),
  chevron: <polyline points="5 8 10 13 15 8" />,
  undo: (
    <>
      <path d="M7.5 11.5 4 8l3.5-3.5" />
      <path d="M4 8h8a5 5 0 0 1 0 10H8.5" />
    </>
  ),
  redo: (
    <>
      <path d="M12.5 11.5 16 8l-3.5-3.5" />
      <path d="M16 8H8a5 5 0 0 0 0 10h3.5" />
    </>
  ),
  theme: (
    <>
      <circle cx="10" cy="10" r="7" />
      <path d="M10 3a7 7 0 0 0 0 14z" fill="currentColor" stroke="none" />
    </>
  ),
  copy: (
    <>
      <rect x="7" y="7" width="9" height="9" rx="1.5" />
      <path d="M4 13V5a1.5 1.5 0 0 1 1.5-1.5H12" />
    </>
  ),
  tidy: (
    <>
      <rect x="3" y="3" width="6" height="14" rx="1" />
      <rect x="11" y="3" width="6" height="9" rx="1" />
    </>
  ),
  play: (
    <path d="M5 3.5v13l11-6.5z" />
  ),
  // A stylised "flow" mark for the wordmark.
  flow: (
    <>
      <path d="M3 6c4 0 4 8 8 8s4-8 8-8" />
      <path d="M3 13c4 0 4 4 8 4" opacity="0.5" />
    </>
  ),
};

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name]}
    </svg>
  );
}
