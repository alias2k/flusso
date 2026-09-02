#!/usr/bin/env bash
# Every `alias2k.github.io/flusso/<path>` URL in the repo's markdown and Rust
# sources must resolve to a page in the freshly built book (docs/book), and a
# `#fragment` must be an element id on that page. The `/schemas/` tree is
# assembled from release tags by pages.yml, not by mdbook, so it is skipped.
#
# Usage: .github/scripts/check-manual-links.sh [book-dir]   (default docs/book)
set -euo pipefail

book="${1:-docs/book}"
[ -d "$book" ] || { echo "no built book at $book — run \`mdbook build docs\` first" >&2; exit 2; }

fail=0
while IFS= read -r line; do
  file="${line%%:*}"
  url="${line#*:}"
  url="${url%.}"
  path="${url#https://alias2k.github.io/flusso}"
  path="${path#/}"
  case "$path" in schemas/*) continue ;; esac
  fragment=""
  case "$path" in *'#'*) fragment="${path#*#}"; path="${path%%#*}" ;; esac
  case "$path" in ""|*/) path="${path}index.html" ;; esac
  target="$book/$path"
  if [ ! -f "$target" ]; then
    echo "::error file=$file::$url → no such page in the built book ($target)"
    fail=1
    continue
  fi
  if [ -n "$fragment" ] && ! grep -q "id=\"$fragment\"" "$target"; then
    echo "::error file=$file::$url → no element with id \"$fragment\" on that page"
    fail=1
  fi
done < <(grep -rHoE --include='*.md' --include='*.rs' --include='*.toml' --include='*.yml' --include='*.yaml' \
           --exclude-dir=target --exclude-dir=node_modules --exclude-dir=book --exclude-dir=dist \
           'https://alias2k\.github\.io/flusso[A-Za-z0-9_./#-]*' . 2>/dev/null \
         | sed -E 's#^\./##' | sort -u)

if [ "$fail" -ne 0 ]; then
  echo "manual deep links are broken (see above)" >&2
  exit 1
fi
echo "manual deep links resolve"
