#!/usr/bin/env bash
# Build the two manuals the Pages site serves (issue #209):
#
#   <site>/        docs/ at the newest release tag of any train (libs, cli, query)
#   <site>/main/   docs/ at main's tip, with a banner and noindex
#
# Each is built from a `git archive` of its whole ref into a temp dir, so the
# checkout is never moved. The overlays in docs/channels/ come from the
# working tree running this script (pages.yml checks out main), so an old
# tag needs no change to carry them.
#
# Usage: .github/scripts/build-manuals.sh <site-dir> [main-ref]   (default origin/main)
set -euo pipefail

site="${1:?usage: build-manuals.sh <site-dir> [main-ref]}"
main_ref="${2:-origin/main}"
mkdir -p "$site"
site="$(cd "$site" && pwd)"
repo="$(git rev-parse --show-toplevel)"
# `git archive` run from a subdirectory archives only that subdirectory.
cd "$repo"
channels="$repo/docs/channels"
work="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/manuals.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# One tag per train is enough: a train's crates release together.
# Prereleases are skipped; the root documents what users install.
release_tags() {
  git for-each-ref --sort=-creatordate --format='%(refname:short)' \
      'refs/tags/flusso-config-v*' 'refs/tags/flusso-cli-v*' 'refs/tags/flusso-query-v*' \
    | grep -E '^flusso-(config|cli|query)-v[0-9]+\.[0-9]+\.[0-9]+$' || true
}

# build <ref> <channel> <site-url> <out-dir>
build() {
  local ref="$1" channel="$2" site_url="$3" out="$4" src="$work/$2"
  mkdir -p "$src"
  # The whole tree, not just docs/: pages `{{#include}}` files from elsewhere
  # in the repo (the script recipes come from libs/2-script/tests/).
  git archive "$ref" | tar -x -C "$src"
  # The overlay is passed as the book's theme dir; a book with its own would lose it.
  if [ -e "$src/docs/theme" ] || grep -qE '^[[:space:]]*theme[[:space:]]*=' "$src/docs/book.toml"; then
    echo "::error::$ref has its own mdBook theme; merge docs/channels/$channel into it before publishing" >&2
    exit 1
  fi
  # mdBook logs a failed `{{#include}}` as ERROR yet exits 0; a page with a
  # hole must not be published, so any ERROR line fails the build.
  NO_COLOR=1 \
  MDBOOK_OUTPUT__HTML__THEME="$channels/$channel" \
  MDBOOK_OUTPUT__HTML__SITE_URL="$site_url" \
    mdbook build "$src/docs" --dest-dir "$out" 2>&1 | tee "$work/$channel.log"
  if grep -qE '(^|[^[:alpha:]])ERROR ' "$work/$channel.log"; then
    echo "::error::mdBook reported errors building $ref (see above)" >&2
    exit 1
  fi
}

release=""
for tag in $(release_tags); do
  if git cat-file -e "${tag}:docs/book.toml" 2>/dev/null; then
    release="$tag"
    break
  fi
  echo "skipping $tag: no docs/book.toml" >&2
done
if [ -z "$release" ]; then
  echo "::error::no release tag carries a buildable docs/; refusing to publish an empty site root" >&2
  exit 1
fi

main_sha="$(git rev-parse --short "$main_ref^{commit}")"
echo "root manual: $release"
echo "main manual: $main_ref ($main_sha)"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  echo "Manual at the root: \`$release\`; under \`/main/\`: \`$main_sha\`" >> "$GITHUB_STEP_SUMMARY"
fi

build "$release" release /flusso/ "$work/root"
if [ -e "$work/root/main" ]; then
  echo "::error::the $release manual has a main/ path, which would shadow the /main/ manual" >&2
  exit 1
fi
cp -r "$work/root/." "$site/"
build "$main_ref" main /flusso/main/ "$site/main"
